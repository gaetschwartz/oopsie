//! Context selector generation for `#[derive(Oopsie)]`.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::ext::IdentExt as _;
use syn::{GenericParam, Generics, Ident, Type, Visibility};

use super::generics::{DeclaredParams, ReferencedParams};
use super::model::{ResolvedEnum, ResolvedStruct};
use super::parse::{CategorizedFields, ModuleSetting, SourceKind, SuffixSetting, UserField};

/// Everything the generators need to emit a selector struct and its impls when
/// the error type is generic. A selector redeclares only the error parameters
/// its captured fields reference (the *projected* set) plus the `__T{i}` `Into`
/// parameters for those fields; its impls additionally carry the error's full
/// parameters and `where` clause, because they name the destination error type
/// (`E<full>`), which mentions every error parameter.
struct SelectorShape<'a> {
    /// The error's full generics, so the impl generators can name the
    /// destination (`E<all>`) and split parameters into selector-bound and free.
    generics: &'a Generics,
    /// Declaration generics for the selector struct: projected error params
    /// (bounds lowered to the impls) followed by the `__T{i}` params, e.g.
    /// `<T, __T0>`.
    struct_decl: TokenStream2,
    /// Use-position generics naming the selector: the same params without
    /// bounds, e.g. `<T, __T0>`. Used wherever the selector type is named (the
    /// `Self`-position of its impls, the `Contextual for Selector<...>` head).
    struct_use: TokenStream2,
    /// The parameters the selector itself carries (projected error params plus
    /// the `__T{i}` `Into` params), every one of which appears in `struct_use`.
    selector_params: Vec<GenericParam>,
    /// The `__T{i}: Into<..>` bounds.
    into_bounds: Vec<TokenStream2>,
    /// The names of the error parameters the selector's fields reference, to
    /// split the error parameters into selector-bound and free.
    referenced_names: std::collections::HashSet<String>,
    /// The braced field block (`{ ... }`), each field carrying its cfg attrs.
    struct_fields: TokenStream2,
    /// The `#[derive(...)]` line for the selector struct.
    derives: TokenStream2,
}

/// Build a selector's generics, field block, and derive set from its user
/// fields, projecting `generics` down to the parameters those fields reference.
/// `doc` renders each field's doc-comment.
///
/// A cfg-gated field cannot ride an `Into` type parameter: cfg attrs are
/// rejected on generic *arguments* (the `Selector<__T>` use position) and
/// unstable in `where` clauses, so a gated `__T` would dangle once the field is
/// stripped. Such fields instead take their own concrete type (no conversion),
/// gated alongside the field; only unconditional fields contribute a parameter.
/// A concrete-typed field also can't ride a blanket `Copy`/`Clone` derive (its
/// type may be neither), so any cfg-gated field drops both from the selector.
fn selector_shape<'a>(
    user_fields: &[UserField],
    generics: &'a Generics,
    doc: &dyn Fn(&UserField) -> String,
) -> SelectorShape<'a> {
    let declared = DeclaredParams::from_generics(generics);
    let mut referenced = ReferencedParams::default();
    let mut into_param_idents = Vec::new();
    let mut into_bounds = Vec::new();
    let mut fields = Vec::new();
    // A selector blanket-derives `Copy`/`Clone` only when every field rides an
    // `Into` parameter; a cfg-gated field (concrete type) or a parameter-typed
    // field (also concrete) may be neither, so either drops both derives.
    let mut all_into_params = true;
    for (i, uf) in user_fields.iter().enumerate() {
        let field_ident = &uf.ident;
        let field_ty = &uf.ty;
        let cfg = &uf.cfg_attrs;
        let field_doc = doc(uf);
        referenced.add_type(field_ty, &declared);
        // A field whose type names a generic parameter takes that parameter's
        // type directly: an `Into`-converted `__T{i}: Into<FieldTy>` would leave
        // the parameter used only in a `where` bound, never in a struct field —
        // which rustc rejects (E0392) and no phantom field can fix without
        // breaking the selector's struct-literal construction. Only a field with
        // no parameter rides the `Into` ergonomics.
        let references_param = declared.type_references_param(field_ty);
        if cfg.is_empty() && !references_param {
            let ty_param = format_ident!("__T{}", i);
            into_param_idents.push(ty_param.clone());
            into_bounds.push(quote! { #ty_param: ::core::convert::Into<#field_ty> });
            fields.push(quote! { #[doc = #field_doc] pub #field_ident: #ty_param });
        } else {
            all_into_params = false;
            fields.push(quote! { #(#cfg)* #[doc = #field_doc] pub #field_ident: #field_ty });
        }
    }

    let projected = super::generics::project(generics, &referenced);
    let into_param_tokens: Vec<TokenStream2> =
        into_param_idents.iter().map(|p| quote! { #p }).collect();
    let struct_decl = join_generics(projected.decl(), &into_param_tokens);
    let struct_use = join_generics(projected.use_(), &into_param_tokens);

    // The selector's own parameters: projected error params (bounds lowered to
    // the impls) plus the synthetic `__T{i}` type params.
    let mut selector_params = projected.params().to_vec();
    for ident in &into_param_idents {
        selector_params.push(syn::parse_quote! { #ident });
    }
    let referenced_names = projected
        .params()
        .iter()
        .map(super::generics::param_name)
        .collect();

    let derives = if all_into_params {
        quote! { #[derive(Debug, Copy, Clone)] }
    } else {
        quote! { #[derive(Debug)] }
    };
    SelectorShape {
        generics,
        struct_decl,
        struct_use,
        selector_params,
        into_bounds,
        referenced_names,
        struct_fields: quote! { { #(#fields),* } },
        derives,
    }
}

impl SelectorShape<'_> {
    /// Impl generics and `where` clause for an impl scoped to the selector alone:
    /// its parameters, the bounds and `where` predicates every one of whose named
    /// parameters the selector carries, and the `Into` bounds. Used for the leaf
    /// `build`/`fail` inherent impl and the `NoSource` `Contextual` impl. A
    /// predicate that also names a *free* parameter is left off here — the leaf
    /// path puts it on `build`/`fail` (which declare the free parameter), and the
    /// `NoSource` impl is emitted only when no parameter is free (see
    /// [`SelectorShape::free_params`]).
    fn selector_impl(&self) -> (TokenStream2, TokenStream2) {
        let params = &self.selector_params;
        let impl_generics = if params.is_empty() {
            quote! {}
        } else {
            quote! { <#(#params),*> }
        };
        let mut predicates = self.scoped_predicates(&self.referenced_names);
        predicates.extend(self.into_bounds.iter().cloned());
        (impl_generics, render_where(&predicates))
    }

    /// Impl generics and `where` clause for a sourced `Contextual<Source>` impl:
    /// every error parameter plus the `__T{i}` params, the full error `where`
    /// clause, and the `Into` bounds. A parameter is constrained only if it
    /// appears in this impl's source type or in this selector's captured fields;
    /// the destination is the trait's associated type, so naming a parameter
    /// there does not constrain it. A parameter constrained by neither (e.g. one
    /// used only by a sibling variant) leaves the impl ill-formed, so callers
    /// reject that configuration via [`SelectorShape::unconstrained_error_param`]
    /// before reaching here.
    fn sourced_impl(&self) -> (TokenStream2, TokenStream2) {
        let mut params: Vec<GenericParam> = self.generics.params.iter().cloned().collect();
        for param in &self.selector_params {
            if super::generics::param_name(param).starts_with("__T") {
                params.push(param.clone());
            }
        }
        let impl_generics = if params.is_empty() {
            quote! {}
        } else {
            quote! { <#(#params),*> }
        };
        let mut predicates: Vec<TokenStream2> = self
            .generics
            .where_clause
            .iter()
            .flat_map(|wc| wc.predicates.iter())
            .map(|pred| quote! { #pred })
            .collect();
        predicates.extend(self.into_bounds.iter().cloned());
        (impl_generics, render_where(&predicates))
    }

    /// The first error parameter a sourced `Contextual<source_type>` impl would
    /// leave unconstrained, if any. A parameter is constrained only when it
    /// appears in `source_type` or in this selector's captured fields; naming it
    /// solely in the impl's associated `Destination` (which every parameter does)
    /// does not count, so rustc would reject the impl with E0207. The selector
    /// cannot escape to a method generic the way the leaf path does, because
    /// `build_error` returns the fixed associated `Destination` — so this whole
    /// configuration is unexpressible and the caller turns it into a clear error.
    /// Const params trigger the same rule; lifetimes are not handled here.
    fn unconstrained_error_param(&self, source_type: &Type) -> Option<&GenericParam> {
        let declared = DeclaredParams::from_generics(self.generics);
        let mut from_source = ReferencedParams::default();
        from_source.add_type(source_type, &declared);
        let constrained = &self.referenced_names | &from_source.names();
        self.generics.params.iter().find(|param| match param {
            GenericParam::Type(_) | GenericParam::Const(_) => {
                !constrained.contains(&super::generics::param_name(param))
            }
            GenericParam::Lifetime(_) => false,
        })
    }

    /// The pool of `where` predicates the leaf impls must place: the error's own
    /// `where` clause plus the inline bounds [`project`] stripped off the
    /// selector's struct parameters (`<U: From<T>>` → `U: From<T>`). A free
    /// parameter's inline bounds are not lowered here — they ride its method-
    /// generic declaration in [`SelectorShape::free_params`] — so each pool
    /// predicate lands on exactly one impl or method.
    ///
    /// [`project`]: super::generics::project
    fn leaf_predicate_pool(&self) -> Vec<syn::WherePredicate> {
        let mut pool =
            super::generics::projected_bound_predicates(self.generics, &self.referenced_names);
        pool.extend(
            self.generics
                .where_clause
                .iter()
                .flat_map(|wc| wc.predicates.iter())
                .cloned(),
        );
        pool
    }

    /// The pool predicates every one of whose named parameters is in `names`, so
    /// they fit an impl scoped to exactly that parameter set. A predicate whose
    /// bound also names a parameter outside `names` (`U: From<T>` with `T` not in
    /// `names`) is held back for whichever method does declare it.
    fn scoped_predicates(&self, names: &std::collections::HashSet<String>) -> Vec<TokenStream2> {
        let declared = DeclaredParams::from_generics(self.generics);
        self.leaf_predicate_pool()
            .iter()
            .filter(|pred| {
                super::generics::predicate_named_params(pred, &declared).is_subset(names)
            })
            .map(|pred| quote! { #pred })
            .collect()
    }

    /// The error parameters no field references, declared with their inline
    /// bounds, plus the pool predicates that name at least one of them, for the
    /// leaf `build`/`fail` methods (which name the destination `E<all>`, bringing
    /// every error parameter into scope). Empty when the selector pins every
    /// error parameter, in which case the `NoSource` `Contextual` impl is emitted.
    fn free_params(&self) -> (Vec<GenericParam>, Vec<TokenStream2>) {
        let decl: Vec<GenericParam> = self
            .generics
            .params
            .iter()
            .filter(|p| {
                !self
                    .referenced_names
                    .contains(&super::generics::param_name(p))
            })
            .cloned()
            .collect();
        let free_names: std::collections::HashSet<String> =
            decl.iter().map(super::generics::param_name).collect();
        let declared = DeclaredParams::from_generics(self.generics);
        let predicates = self
            .leaf_predicate_pool()
            .iter()
            .filter(|pred| {
                !super::generics::predicate_named_params(pred, &declared).is_disjoint(&free_names)
            })
            .map(|pred| quote! { #pred })
            .collect();
        (decl, predicates)
    }
}

/// Splice `extra` parameters onto a `<...>` (or empty) generic list, producing a
/// single bracketed list or empty tokens when both sides are empty. Keeps the
/// projected error params first so declaration order is stable.
fn join_generics(base: TokenStream2, extra: &[TokenStream2]) -> TokenStream2 {
    let base_empty = base.is_empty();
    match (base_empty, extra.is_empty()) {
        (true, true) => quote! {},
        (true, false) => quote! { <#(#extra),*> },
        (false, true) => base,
        (false, false) => {
            // `base` already renders as `<...>`; strip its brackets so the
            // merged list stays a single pair.
            let inner = strip_angle_brackets(&base);
            quote! { <#inner, #(#extra),*> }
        }
    }
}

/// Remove the outer `<` `>` from a rendered generic list so two lists can be
/// concatenated into one. The input is always the `quote!` output of a
/// non-empty `<...>`, so the first and last tokens are the angle brackets.
fn strip_angle_brackets(generics: &TokenStream2) -> TokenStream2 {
    let mut iter = generics.clone().into_iter();
    // Drop leading `<`.
    iter.next();
    let mut collected: Vec<proc_macro2::TokenTree> = iter.collect();
    // Drop trailing `>`.
    collected.pop();
    collected.into_iter().collect()
}

/// Render a list of `where` predicates into a `where ...` clause, or empty
/// tokens when the list is empty.
fn render_where(predicates: &[TokenStream2]) -> TokenStream2 {
    if predicates.is_empty() {
        quote! {}
    } else {
        quote! { where #(#predicates),* }
    }
}

/// Per-field initializers for the destination's struct expression, e.g.
/// `name: self.name.into()`. A field that keeps its concrete type on the
/// selector — cfg-gated, or a generic parameter named directly — already holds
/// the destination type, so it moves without `.into()`; cfg attrs ride along so
/// the initializer is stripped together with the field.
fn user_init_exprs(user_fields: &[UserField], generics: &syn::Generics) -> Vec<TokenStream2> {
    let declared = DeclaredParams::from_generics(generics);
    user_fields
        .iter()
        .map(|uf| {
            let ident = &uf.ident;
            let cfg = &uf.cfg_attrs;
            if cfg.is_empty() && !declared.type_references_param(&uf.ty) {
                quote! { #ident: self.#ident.into() }
            } else {
                quote! { #(#cfg)* #ident: self.#ident }
            }
        })
        .collect()
}

/// Resolve a selector's visibility from an explicit `vis(...)` override or, in
/// its absence, the error type's own visibility (snafu parity: a library's
/// `pub` error yields selectors its downstream users can name). Both sources go
/// through the same child-module lift, so an explicit override can't drift the
/// way it did when only the default path lifted.
fn resolve_selector_vis(
    explicit: Option<&Visibility>,
    error_vis: &Visibility,
    wrapped_in_module: bool,
) -> Visibility {
    let chosen = explicit.unwrap_or(error_vis);
    if wrapped_in_module {
        lift_into_child_module(chosen)
    } else {
        chosen.clone()
    }
}

/// Re-express a visibility one module level deeper, as seen from a generated
/// child module that holds the selectors. `pub` and crate-absolute paths are
/// position-independent; module-relative ones (`super`, `self`, `in path`) and
/// inherited (private) visibility gain a `super` so they still reach the
/// error's own scope.
fn lift_into_child_module(vis: &Visibility) -> Visibility {
    let restricted = match vis {
        Visibility::Public(_) => return vis.clone(),
        Visibility::Inherited => return syn::parse_quote! { pub(super) },
        Visibility::Restricted(restricted) => restricted,
    };
    let path = &restricted.path;
    match path.segments.first() {
        Some(seg) if seg.ident == "crate" => vis.clone(),
        Some(seg) if seg.ident == "self" && path.segments.len() == 1 => {
            syn::parse_quote! { pub(super) }
        }
        _ => syn::parse_quote! { pub(in super::#path) },
    }
}

/// Generate context selectors for all variants of an enum.
pub fn gen_enum_selectors(
    resolved: &ResolvedEnum,
    oopsie_path: &syn::Path,
) -> syn::Result<Vec<TokenStream2>> {
    let input = resolved.input;
    let container = resolved.container;
    let enum_ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let wrapped_in_module = matches!(container.effective_module(), ModuleSetting::On(_));
    let vis = resolve_selector_vis(container.visibility(), &input.vis, wrapped_in_module);

    let mut selectors = Vec::new();
    for v in &resolved.variants {
        let variant_attrs = &v.attrs;
        let categorized = &v.fields;
        let variant_ident = v.ident();
        let cfg_attrs = &v.cfg_attrs;

        let Some(selector_ident) = &v.selector_ident else {
            // `transparent` variant: the resolved model validated the source
            // shape, so emit the `From` impl directly. When the source uses
            // `from(T, transform)` (or auto-box detected `Box<T>`), the `From`
            // accepts the pre-transform type `T` and applies the transform
            // internally, matching snafu's `#[snafu(context(false))]`.
            let source = categorized
                .source
                .as_ref()
                .expect("transparent variant has a validated source");
            let source_ident = &source.ident;
            let (param_ty, body_assign) = match &source.kind {
                SourceKind::Transformed {
                    source_type,
                    transform,
                } => (
                    quote! { #source_type },
                    quote! { let #source_ident = (#transform)(source); },
                ),
                SourceKind::Yes => {
                    let ty = &source.ty;
                    (quote! { #ty }, quote! { let #source_ident = source; })
                }
                SourceKind::No | SourceKind::Disabled => {
                    unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
                }
            };
            let auto_inits = gen_auto_inits(categorized, oopsie_path, true);
            let auto_names = gen_auto_field_inits(categorized);
            selectors.push(quote! {
                #(#cfg_attrs)*
                impl #impl_generics ::core::convert::From<#param_ty> for #enum_ident #ty_generics #where_clause {
                    #[track_caller]
                    fn from(source: #param_ty) -> Self {
                        // Capture probes borrow `&source` before
                        // `body_assign` moves it into the renamed field.
                        #(#auto_inits)*
                        #body_assign
                        #enum_ident::#variant_ident {
                            #source_ident,
                            #(#auto_names)*
                        }
                    }
                }
            });
            continue;
        };

        // `vis` is already re-anchored to the child module, so the fallback
        // takes no further lift; an explicit variant override goes through the
        // same lift as the container default rather than being emitted verbatim.
        let selector_vis = match variant_attrs.visibility() {
            Some(explicit) => resolve_selector_vis(Some(explicit), &vis, wrapped_in_module),
            None => vis.clone(),
        };

        let has_source = categorized.source.is_some();
        let user_fields = &categorized.user_fields;

        // Generate selector struct
        let shape = selector_shape(user_fields, &input.generics, &|uf| {
            format!(
                "Value for the `{}` field of `{enum_ident}::{variant_ident}`.",
                uf.ident
            )
        });
        let SelectorShape {
            struct_decl,
            struct_fields,
            derives,
            ..
        } = &shape;

        let selector_doc = format!("Context selector for `{enum_ident}::{variant_ident}`.");
        let selector_struct = if user_fields.is_empty() {
            quote! {
                #(#cfg_attrs)*
                #[doc = #selector_doc]
                #[derive(Debug, Copy, Clone)]
                #selector_vis struct #selector_ident;
            }
        } else {
            quote! {
                #(#cfg_attrs)*
                #[doc = #selector_doc]
                #derives
                #selector_vis struct #selector_ident #struct_decl #struct_fields
            }
        };

        // The destination error type as seen from the selector's scope, carrying
        // its type parameters so a generic enum's variant resolves. The variant
        // is constructed through the unqualified `E::Variant` path so inference
        // fills the type parameters from the field values and the return type.
        let dest_ty = quote! { #enum_ident #ty_generics };
        let construct = quote! { #enum_ident::#variant_ident };
        let doc_name = format!("{enum_ident}::{}", variant_ident.unraw());
        let dest = Destination {
            ty: &dest_ty,
            construct: &construct,
            doc_name: &doc_name,
            generics: &input.generics,
        };

        // Generate Contextual or build/fail depending on whether there's a source
        let methods_inner = if has_source {
            reject_unconstrained_sourced(categorized, &shape)?;
            gen_build_error(selector_ident, &dest, categorized, &shape, oopsie_path)
        } else {
            gen_build_fail(selector_ident, &dest, categorized, &shape, oopsie_path)
        };
        // Wrap methods in `const _: () = { ... };` so cfg-attrs apply to all
        // impl blocks emitted by gen_build_error / gen_build_fail.
        let methods = if cfg_attrs.is_empty() {
            methods_inner
        } else {
            quote! {
                #(#cfg_attrs)*
                const _: () = {
                    #methods_inner
                };
            }
        };

        selectors.push(quote! {
            #selector_struct
            #methods
        });
    }

    Ok(selectors)
}

/// Generate context selector for a struct error.
pub fn gen_struct_selector(
    resolved: &ResolvedStruct,
    oopsie_path: &syn::Path,
) -> syn::Result<TokenStream2> {
    let input = resolved.input;
    let attrs = resolved.attrs;
    let struct_ident = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let wrapped_in_module = matches!(attrs.container.effective_module(), ModuleSetting::On(_));
    let vis = resolve_selector_vis(attrs.visibility(), &input.vis, wrapped_in_module);
    // The selector and the error type can share a name (struct not ending in
    // `Error`, default `Off` suffix). Inside the generated module the selector
    // would then shadow the `use super::*`-imported type, so every reference to
    // the destination type goes through `super::`. A `transparent` struct emits
    // its `From` impl unwrapped, so it always names the type directly. The error
    // type's parameters ride along so a generic struct's destination resolves.
    let dest_ty: TokenStream2 = if wrapped_in_module {
        quote! { super::#struct_ident #ty_generics }
    } else {
        quote! { #struct_ident #ty_generics }
    };
    // The struct is built through its bare path so inference fills the type
    // parameters from the field values and the return type; spelling them in a
    // struct-literal head (`Foo<T> { .. }`) is a parse error.
    let construct: TokenStream2 = if wrapped_in_module {
        quote! { super::#struct_ident }
    } else {
        quote! { #struct_ident }
    };
    // Variant-level fields are inlined on `StructAttrs` (darling allows only
    // one flatten per derive); aliasing makes downstream field access read
    // naturally as `variant_attrs.transparent` etc.
    let variant_attrs = attrs;

    let categorized = &resolved.fields;

    if variant_attrs.transparent {
        // The resolved model validated the source shape for a transparent struct.
        let source = categorized
            .source
            .as_ref()
            .expect("transparent struct has a validated source");
        let source_ident = &source.ident;
        let (param_ty, body_assign) = match &source.kind {
            SourceKind::Transformed {
                source_type,
                transform,
            } => (
                quote! { #source_type },
                quote! { let #source_ident = (#transform)(source); },
            ),
            SourceKind::Yes => {
                let ty = &source.ty;
                (quote! { #ty }, quote! { let #source_ident = source; })
            }
            SourceKind::No | SourceKind::Disabled => {
                unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
            }
        };
        let auto_inits = gen_auto_inits(categorized, oopsie_path, true);
        let auto_names = gen_auto_field_inits(categorized);
        let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
        return Ok(quote! {
            impl #impl_generics ::core::convert::From<#param_ty> for #struct_ident #ty_generics #where_clause {
                #[track_caller]
                fn from(source: #param_ty) -> Self {
                    // Capture probes borrow `&source` before `body_assign`
                    // moves it into the renamed field.
                    #(#auto_inits)*
                    #body_assign
                    Self { #source_ident, #(#auto_names)* }
                }
            }
        });
    }

    let selector_ident = selector_name(struct_ident, &attrs.container.effective_suffix())?;
    let has_source = categorized.source.is_some();
    let user_fields = &categorized.user_fields;

    let shape = selector_shape(user_fields, &input.generics, &|uf| {
        format!("Value for the `{}` field of `{struct_ident}`.", uf.ident)
    });
    let SelectorShape {
        struct_decl,
        struct_fields,
        derives,
        ..
    } = &shape;

    let selector_doc = format!("Context selector for `{struct_ident}`.");
    let selector_struct = if user_fields.is_empty() {
        quote! {
            #[doc = #selector_doc]
            #[derive(Debug, Copy, Clone)]
            #vis struct #selector_ident;
        }
    } else {
        quote! {
            #[doc = #selector_doc]
            #derives
            #vis struct #selector_ident #struct_decl #struct_fields
        }
    };

    let doc_name = struct_ident.unraw().to_string();
    let dest = Destination {
        ty: &dest_ty,
        construct: &construct,
        doc_name: &doc_name,
        generics: &input.generics,
    };
    let methods = if has_source {
        reject_unconstrained_sourced(categorized, &shape)?;
        gen_build_error(&selector_ident, &dest, categorized, &shape, oopsie_path)
    } else {
        gen_build_fail(&selector_ident, &dest, categorized, &shape, oopsie_path)
    };

    Ok(quote! {
        #selector_struct
        #methods
    })
}

pub(super) fn selector_name(base: &Ident, suffix: &SuffixSetting) -> syn::Result<Ident> {
    let base_str = base.unraw().to_string();
    let stripped = base_str
        .strip_suffix("Error")
        .filter(|s| !s.is_empty())
        .unwrap_or(&base_str);
    let name = match suffix {
        SuffixSetting::Off => stripped.to_owned(),
        SuffixSetting::Custom(s) => format!("{stripped}{s}"),
    };
    ident_maybe_raw(&name, base.span())
}

/// Derived names can collide with keywords (`r#try` strips to `try`), which
/// `Ident::new` panics on; those become raw idents. The handful of names that
/// cannot be raw either are a real error, not a panic.
fn ident_maybe_raw(name: &str, span: proc_macro2::Span) -> syn::Result<Ident> {
    match syn::parse_str::<Ident>(name) {
        Ok(mut id) => {
            id.set_span(span);
            Ok(id)
        }
        Err(_) if matches!(name, "self" | "Self" | "super" | "crate" | "_") => {
            Err(syn::Error::new(
                span,
                format!(
                    "cannot generate a selector named `{name}`; rename the item or set \
                 `#[oopsie(suffix = \"...\")]`"
                ),
            ))
        }
        Err(_) => Ok(Ident::new_raw(name, span)),
    }
}

fn gen_auto_inits(
    categorized: &CategorizedFields,
    oopsie_path: &syn::Path,
    has_source: bool,
) -> Vec<TokenStream2> {
    categorized
        .auto_fields
        .iter()
        .map(|af| {
            let ident = &af.ident;
            let ty = &af.ty;
            let cfg = &af.cfg_attrs;
            if has_source {
                quote! {
                    #(#cfg)*
                    let #ident = {
                        use #oopsie_path::__private::{CaptureFromExt as _, CaptureFromFallback as _};
                        (&#oopsie_path::__private::CaptureProbe(&source)).resolve::<#ty>()
                    };
                }
            } else {
                quote! { #(#cfg)* let #ident = <#ty as #oopsie_path::Capturable>::capture(); }
            }
        })
        .collect()
}

/// Shorthand struct-expression initializers (`name,`) for auto-captured fields,
/// each carrying its cfg attrs so a stripped field's binding and reference
/// vanish together.
fn gen_auto_field_inits(categorized: &CategorizedFields) -> Vec<TokenStream2> {
    categorized
        .auto_fields
        .iter()
        .map(|af| {
            let ident = &af.ident;
            let cfg = &af.cfg_attrs;
            quote! { #(#cfg)* #ident, }
        })
        .collect()
}

/// The destination error a selector builds, as the build generators need it.
struct Destination<'a> {
    /// The destination type as seen from the selector's scope
    /// (`super::`-qualified for a module-wrapped struct, carrying type
    /// parameters when the error is generic).
    ty: &'a TokenStream2,
    /// The path the destination is built through — `E::Variant` for an enum, the
    /// bare type path for a struct (no generic arguments, which would be a
    /// struct-literal parse error).
    construct: &'a TokenStream2,
    /// The destination's display name for generated docs.
    doc_name: &'a str,
    /// The error type's generics, to tell a parameter-typed field (moved as-is)
    /// from an `Into`-converted one (`.into()`).
    generics: &'a syn::Generics,
}

/// The type spelled in a sourced selector's `Contextual<...>` head — the source
/// field's declared type, or the pre-transform type for a `from(Type, ..)`
/// source. Used to decide which error parameters the impl constrains.
fn sourced_impl_source_type(source_field: &super::parse::SourceField) -> &Type {
    match &source_field.kind {
        SourceKind::No | SourceKind::Disabled => {
            unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
        }
        SourceKind::Yes => &source_field.ty,
        SourceKind::Transformed { source_type, .. } => source_type,
    }
}

/// Reject a sourced variant whose `Contextual` impl would leave an error
/// parameter unconstrained (rustc E0207). Such a parameter is used only by
/// sibling variants, so it cannot ride this selector's source type or fields nor
/// move to a method generic (the impl's associated `Destination` is fixed).
fn reject_unconstrained_sourced(
    categorized: &CategorizedFields,
    shape: &SelectorShape,
) -> syn::Result<()> {
    let source_field = categorized
        .source
        .as_ref()
        .expect("sourced selector has a source field");
    if let Some(param) = shape.unconstrained_error_param(sourced_impl_source_type(source_field)) {
        let name = super::generics::param_name(param);
        return Err(syn::Error::new_spanned(
            param,
            format!(
                "a variant with a `source` field must reference every generic parameter of the \
                 error type in its source type or its other fields; parameter `{name}` is used \
                 only by other variants, which would leave this selector's `Contextual` impl \
                 unconstrained — split it into its own error type or add `{name}` to this variant"
            ),
        ));
    }
    Ok(())
}

/// Generate the `Contextual` impl for a sourced selector.
///
/// The impl carries the error's full generics and `where` clause (it names the
/// destination), merged with the selector's `Into` parameters and bounds; the
/// selector is named through its projected use-generics.
fn gen_build_error(
    selector_ident: &Ident,
    dest: &Destination,
    categorized: &CategorizedFields,
    shape: &SelectorShape,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let dest_ty = dest.ty;
    let construct = dest.construct;
    let generics = dest.generics;
    let source_field = categorized.source.as_ref().unwrap();
    let source_ident = &source_field.ident;

    let (source_type, source_transform) = match &source_field.kind {
        SourceKind::No | SourceKind::Disabled => {
            unreachable!("categorized.source set but kind is SourceKind::No or Disabled")
        }
        SourceKind::Yes => {
            let ty = &source_field.ty;
            (quote! { #ty }, None)
        }
        SourceKind::Transformed {
            source_type,
            transform,
        } => (quote! { #source_type }, Some(quote! { (#transform) })),
    };

    let auto_inits = gen_auto_inits(categorized, oopsie_path, true);
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields, generics);

    let source_assign = if let Some(transform) = source_transform {
        quote! { let #source_ident = #transform(source); }
    } else {
        quote! { let #source_ident = source; }
    };

    let struct_use = &shape.struct_use;
    let (impl_generics, impl_where) = shape.sourced_impl();
    quote! {
        impl #impl_generics #oopsie_path::Contextual<#source_type> for #selector_ident #struct_use
        #impl_where
        {
            type Destination = #dest_ty;

            #[track_caller]
            fn build_error(self, source: #source_type) -> #dest_ty {
                // Capture probes borrow `&source`, so they must run before
                // `source_assign` moves `source` into the (possibly renamed)
                // field. They also see the pre-transform value, preserving the
                // source's own trace.
                #(#auto_inits)*
                #source_assign
                #construct {
                    #(#user_inits,)*
                    #source_ident,
                    #(#auto_names)*
                }
            }
        }
    }
}

/// Generate `build()`/`fail()` plus the `NoSource` `Contextual` impl for a leaf
/// selector (no source).
fn gen_build_fail(
    selector_ident: &Ident,
    dest: &Destination,
    categorized: &CategorizedFields,
    shape: &SelectorShape,
    oopsie_path: &syn::Path,
) -> TokenStream2 {
    let dest_ty = dest.ty;
    let construct = dest.construct;
    let doc_name = dest.doc_name;
    let generics = dest.generics;
    let auto_inits = gen_auto_inits(categorized, oopsie_path, false);
    let auto_names = gen_auto_field_inits(categorized);
    let user_inits = user_init_exprs(&categorized.user_fields, generics);

    let struct_use = &shape.struct_use;
    let (impl_generics, impl_where) = shape.selector_impl();

    // A leaf selector that does not reference every error parameter cannot pin
    // the destination's type arguments: `build`/`fail` take the remaining
    // (free) parameters as method generics, inferred from the caller's return
    // annotation. The `NoSource` `Contextual` impl, whose `Destination`
    // associated type must be fully concrete, exists only when no parameter is
    // free — otherwise the selector pins every parameter and the impl is exact.
    let (free_decl, free_preds) = shape.free_params();
    let build_generics = if free_decl.is_empty() {
        quote! {}
    } else {
        quote! { <#(#free_decl),*> }
    };
    let build_where = render_where(&free_preds);
    // `fail` adds its own `Ok`-type parameter alongside the destination's free
    // ones; both sets carry the destination's `where` predicates.
    let fail_generics = if free_decl.is_empty() {
        quote! { <__T> }
    } else {
        // Lifetimes must precede type/const params: emit free lifetimes, then the
        // `Ok`-type `__T`, then the remaining free params.
        let (free_lts, free_rest): (Vec<_>, Vec<_>) = free_decl
            .iter()
            .partition(|p| matches!(p, GenericParam::Lifetime(_)));
        quote! { <#(#free_lts,)* __T, #(#free_rest),*> }
    };
    let fail_where = render_where(&free_preds);

    let none_error_impl = if free_decl.is_empty() {
        quote! {
            impl #impl_generics #oopsie_path::Contextual<#oopsie_path::NoSource> for #selector_ident #struct_use
            #impl_where
            {
                type Destination = #dest_ty;

                #[track_caller]
                fn build_error(self, _: #oopsie_path::NoSource) -> #dest_ty {
                    self.build()
                }
            }
        }
    } else {
        quote! {}
    };

    let build_doc = format!("Builds `{doc_name}` from this selector's fields.");
    let fail_doc = format!("Builds `{doc_name}` and returns it as `Err`.");
    quote! {
        impl #impl_generics #selector_ident #struct_use
        #impl_where
        {
            #[doc = #build_doc]
            #[must_use]
            #[track_caller]
            pub fn build #build_generics (self) -> #dest_ty #build_where {
                #(#auto_inits)*
                #construct {
                    #(#user_inits,)*
                    #(#auto_names)*
                }
            }

            #[doc = #fail_doc]
            #[track_caller]
            pub fn fail #fail_generics (self) -> ::core::result::Result<__T, #dest_ty> #fail_where {
                ::core::result::Result::Err(self.build())
            }
        }

        #none_error_impl
    }
}
