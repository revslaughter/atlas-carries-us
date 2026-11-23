//! A stand-in for axum: a router, method routers and handlers, with the
//! shapes of axum's API and no server.

use std::future::Future;
use std::marker::PhantomData;

pub mod extract {
    //! Request extractors.

    /// A path parameter.
    pub struct Path<T>(pub T);

    /// A JSON body.
    pub struct Json<T>(pub T);
}

pub mod routing {
    //! Routers and method routers.

    use super::*;

    /// A function that can handle a request.
    pub trait Handler<T> {}

    impl<F, Fut> Handler<()> for F
    where
        F: FnOnce() -> Fut,
        Fut: Future,
    {
    }

    impl<F, Fut, A> Handler<(A,)> for F
    where
        F: FnOnce(A) -> Fut,
        Fut: Future,
    {
    }

    /// Routes requests by path.
    pub struct Router<S = ()> {
        state: PhantomData<S>,
    }

    impl<S> Router<S> {
        /// Makes an empty router.
        pub fn new() -> Self {
            Router { state: PhantomData }
        }

        /// Adds a route.
        pub fn route(self, _path: &str, _method_router: MethodRouter<S>) -> Self {
            self
        }

        /// Adds a service that answers every method on one path.
        pub fn route_service<T>(self, _path: &str, _service: T) -> Self {
            self
        }

        /// Mounts another router under a path.
        pub fn nest(self, _path: &str, _router: Router<S>) -> Self {
            self
        }

        /// Adds the routes of another router.
        pub fn merge(self, _other: Router<S>) -> Self {
            self
        }
    }

    impl<S> Default for Router<S> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Routes requests by method.
    pub struct MethodRouter<S = ()> {
        state: PhantomData<S>,
    }

    impl<S> MethodRouter<S> {
        /// Adds a service for POST.
        pub fn post_service<T>(self, _service: T) -> Self {
            self
        }
    }

    macro_rules! verbs {
        ($($name:ident),*) => {
            impl<S> MethodRouter<S> {
                $(
                    /// Adds a handler for one method.
                    pub fn $name<H: Handler<T>, T>(self, _handler: H) -> Self {
                        self
                    }
                )*
            }
            $(
                /// Makes a method router with a handler for one method.
                pub fn $name<H: Handler<T>, T, S>(_handler: H) -> MethodRouter<S> {
                    MethodRouter { state: PhantomData }
                }
            )*
        };
    }

    verbs!(get, post, put, delete, patch, any);
}

pub use routing::Router;
