#![cfg_attr(feature = "unstable-error-generic-member-access", feature(error_generic_member_access))]

use oopsie::Oopsie;

#[derive(Debug, Clone)]
pub struct User {
    pub id: u32,
}

#[derive(Debug, Oopsie)]
pub enum AppError {
    #[oopsie("user {id} missing")]
    User { id: u32 },
    #[oopsie("denied")]
    Denied { user: User },
}

pub trait Policy {}

#[derive(Debug, Oopsie)]
#[oopsie(module(false))]
pub enum PolicyError<T>
where
    T: Policy + std::fmt::Debug,
{
    #[oopsie("policy")]
    Policy { owner: Option<Vec<T>> },
}

#[derive(Debug, Oopsie)]
#[oopsie(module, suffix(false))]
pub struct Session {
    pub parent: Option<Box<Session>>,
}

#[derive(Debug, Clone)]
pub struct Nested;

#[derive(Debug, Oopsie)]
pub enum SelfPathError {
    #[oopsie("nested")]
    Nested { inner: self::Nested },
}

fn main() {}
