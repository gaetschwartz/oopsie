//! Iterating an error and its transitive [`source`](std::error::Error::source) causes.

use core::error::Error as StdError;

/// An iterator over an error and its chain of [`source`](StdError::source) causes.
///
/// Yields the error itself first, then each successive `source()`, matching
/// `anyhow::Chain`: `err.chain().next()` is `Some(err)`, not its first cause.
/// Build one with [`ErrorChainExt::chain`].
///
/// Unlike the depth-capped walks elsewhere in the crate, this iterator is an
/// honest cursor with no internal bound: a consumer caps it (`.take(n)`,
/// `.find(..)`). A source chain that cycles back on itself — permitted by the
/// `Error` contract — makes the iterator loop forever, the same as
/// `anyhow::Chain`.
#[derive(Clone, Debug)]
pub struct Chain<'a> {
    next: Option<&'a (dyn StdError + 'static)>,
}

impl<'a> Chain<'a> {
    #[inline]
    fn new(head: &'a (dyn StdError + 'static)) -> Self {
        Self { next: Some(head) }
    }
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn StdError + 'static);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = current.source();
        Some(current)
    }
}

impl core::iter::FusedIterator for Chain<'_> {}

/// Walk an error and its [`source`](StdError::source) chain.
///
/// Import via `use oopsie::prelude::*` or `use oopsie::ErrorChainExt`.
///
/// Implemented for every `E: Error + 'static` and for `dyn Error + 'static`
/// itself, so it covers a typed `#[oopsie]` error, a [`Welp`](crate::Welp), and
/// the `&(dyn Error + 'static)` returned by [`source`](StdError::source) with a
/// single method call.
///
/// # Example
///
/// ```
/// use oopsie::{Oopsie, ErrorChainExt as _, ResultExt as _};
///
/// #[derive(Debug, Oopsie)]
/// #[oopsie(module(false))]
/// enum ReadError {
///     #[oopsie("read failed: {source}")]
///     Read { source: std::io::Error },
/// }
///
/// # fn main() {
/// let err: Result<(), std::io::Error> = Err(std::io::Error::other("disk full"));
/// let err = err.context(Read).unwrap_err();
///
/// let messages: Vec<String> = err.chain().map(ToString::to_string).collect();
/// assert_eq!(messages, ["read failed: disk full", "disk full"]);
/// assert_eq!(err.root_cause().to_string(), "disk full");
/// # }
/// ```
pub trait ErrorChainExt {
    /// Iterate this error followed by its transitive
    /// [`source`](StdError::source) causes.
    ///
    /// The first item is the error itself ([`anyhow::Chain`] semantics), so the
    /// chain is never empty.
    ///
    /// [`anyhow::Chain`]: https://docs.rs/anyhow/latest/anyhow/struct.Chain.html
    fn chain(&self) -> Chain<'_>;

    /// The last error in the chain: the deepest [`source`](StdError::source),
    /// or this error itself when it has none.
    ///
    /// Unlike [`chain`](Self::chain), this walk is capped at
    /// `MAX_SOURCE_CHAIN_DEPTH` hops, so a `source()` cycle (permitted by the
    /// `Error` contract) yields the node at the cap instead of spinning
    /// forever.
    fn root_cause(&self) -> &(dyn StdError + 'static);
}

/// Matches the cap used elsewhere in the crate (report.rs, erased/mod.rs) for
/// consistency, though this walk has no serialized representation to bound.
const MAX_SOURCE_CHAIN_DEPTH: usize = 128;

/// The shared body, taking an already-erased head so both the blanket impl and
/// the `dyn Error` impl reuse it — a `?Sized` blanket can't, since `&E` only
/// unsizes to `&dyn Error` when `E: Sized`.
#[inline]
fn root_cause_of<'a>(head: &'a (dyn StdError + 'static)) -> &'a (dyn StdError + 'static) {
    let mut cause = head;
    for _ in 0..MAX_SOURCE_CHAIN_DEPTH {
        let Some(source) = cause.source() else {
            break;
        };
        cause = source;
    }
    cause
}

impl<E: StdError + 'static> ErrorChainExt for E {
    #[inline]
    fn chain(&self) -> Chain<'_> {
        Chain::new(self)
    }

    #[inline]
    fn root_cause(&self) -> &(dyn StdError + 'static) {
        root_cause_of(self)
    }
}

impl ErrorChainExt for dyn StdError + 'static {
    #[inline]
    fn chain(&self) -> Chain<'_> {
        Chain::new(self)
    }

    #[inline]
    fn root_cause(&self) -> &(dyn StdError + 'static) {
        root_cause_of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Debug)]
    struct Leaf;

    impl fmt::Display for Leaf {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("leaf")
        }
    }

    impl StdError for Leaf {}

    #[derive(Debug)]
    struct Layer {
        label: &'static str,
        source: Box<dyn StdError + 'static>,
    }

    impl fmt::Display for Layer {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.label)
        }
    }

    impl StdError for Layer {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(&*self.source)
        }
    }

    fn three_deep() -> Layer {
        Layer {
            label: "top",
            source: Box::new(Layer {
                label: "middle",
                source: Box::new(Leaf),
            }),
        }
    }

    #[test]
    fn chain_starts_at_self_then_walks_sources() {
        let err = three_deep();
        let labels: Vec<String> = err.chain().map(ToString::to_string).collect();
        assert_eq!(labels, ["top", "middle", "leaf"]);
    }

    #[test]
    fn chain_on_leaf_yields_only_self() {
        let leaf = Leaf;
        assert_eq!(leaf.chain().count(), 1);
        assert_eq!(leaf.chain().next().unwrap().to_string(), "leaf");
    }

    #[test]
    fn chain_over_dyn_error_from_source_starts_at_that_source() {
        let err = three_deep();
        let source = err.source().expect("has a source");
        let labels: Vec<String> = source.chain().map(ToString::to_string).collect();
        assert_eq!(labels, ["middle", "leaf"]);
    }

    #[test]
    fn root_cause_is_the_deepest_link() {
        let err = three_deep();
        let root = err.root_cause();
        assert_eq!(root.to_string(), "leaf");
        // Identity: the root is the same value the last chain link points at.
        let last = err.chain().last().expect("non-empty chain");
        assert!(std::ptr::eq(
            std::ptr::from_ref(root).cast::<()>(),
            std::ptr::from_ref(last).cast::<()>(),
        ));
    }

    #[test]
    fn root_cause_of_leaf_is_itself() {
        let leaf = Leaf;
        assert_eq!(leaf.root_cause().to_string(), "leaf");
    }

    #[derive(Debug)]
    struct SelfCycle;

    impl fmt::Display for SelfCycle {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("cycle")
        }
    }

    impl StdError for SelfCycle {
        fn source(&self) -> Option<&(dyn StdError + 'static)> {
            Some(self)
        }
    }

    #[test]
    fn root_cause_terminates_on_cyclic_source_chain() {
        let err = SelfCycle;
        let root = err.root_cause();
        assert_eq!(root.to_string(), "cycle");
    }
}
