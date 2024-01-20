// Macros to avoid repeating myself when writing code.

#[macro_export]
macro_rules! define_database {
    ($struct_name: ident, $name: literal) => {
        pub struct $struct_name;

        rocket_sqlite_rw_pool::paste::paste! {
            impl $struct_name {
                pub fn fairing() -> impl rocket::fairing::Fairing {
                    let initializers: Vec<_> = rocket_sqlite_rw_pool::inventory::iter::<
                        rocket_sqlite_rw_pool::PoolInitializer<$struct_name>,
                    >
                        .map(rocket_sqlite_rw_pool::TypeErasedPoolInitializer::from)
                        .collect();
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::fairing(
                        "'#name' Database Pool",
                        $name,
                        initializers,
                    )
                }

                pub async fn get_one<'rocket, P: rocket::Phase>(
                    rocket: &'rocket rocket::Rocket<P>,
                ) -> Option<rocket_sqlite_rw_pool::Connector<'rocket, Self>> {
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::get_one(&rocket).await
                }

                pub fn pool<P: rocket::Phase>(
                    rocket: &rocket::Rocket<P>,
                ) -> Option<&rocket_sqlite_rw_pool::ConnectionPool<Self>> {
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::get_pool(&rocket)
                }
            }

            pub struct [<$struct_name _Initializer>] {
                initializer: rocket_sqlite_rw_pool::PoolInitializerFn
            }

            impl [<$struct_name _Initializer>] {
                pub const fn new(initializer: rocket_sqlite_rw_pool::PoolInitializerFn) -> Self {
                    Self {
                        initializer
                    }
                }
            }

            impl From<&'static [<$struct_name _Initializer>]> for rocket_sqlite_rw_pool::TypeErasedPoolInitializer {
                fn from(initializer: &'static [<$struct_name _Initializer>]) -> Self {
                    Self {
                        initializer: initializer.initializer,
                    }
                }
            }

            rocket_sqlite_rw_pool::inventory::collect!(
                [<$struct_name _Initializer>]
            );
        }
    };

    ($struct_name: ident, $name: literal, $migrations: literal) => {
        // Callers will likely re-export this.
        // RustEmbed needs the same name method
        #[allow(clippy::redundant_pub_crate, clippy::same_name_method)]
        pub(crate) mod migrations {
            #[allow(non_snake_case)]
            pub(crate) mod $struct_name {
                #[macro_use]
                use rocket_sqlite_rw_pool::rust_embed;
                use rust_embed::RustEmbed;

                #[derive(RustEmbed)]
                #[folder = $migrations]
                #[include = "*.sql"]
                pub(crate) struct Migrations;
            }
        }

        pub struct $struct_name;

        rocket_sqlite_rw_pool::paste::paste! {
            impl $struct_name {
                pub fn fairing() -> impl rocket::fairing::Fairing {
                    // TODO: Verify the name shows right
                    const FAIRING_NAME: &'static str = concat!($name, " Database Pool");
                    let initializers: Vec<_> = rocket_sqlite_rw_pool::inventory::iter::<
                        [<$struct_name _Initializer>],
                    >()
                        .map(rocket_sqlite_rw_pool::PoolInitializer::from)
                        .collect();
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::fairing_with_migrations::<
                        migrations::$struct_name::Migrations,
                    >(FAIRING_NAME, $name, initializers)
                }

                pub fn get_one<'rocket, P: rocket::Phase>(
                    rocket: &'rocket rocket::Rocket<P>,
                ) -> Option<rocket_sqlite_rw_pool::Connector<'rocket, Self>> {
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::get_one(&rocket)
                }

                pub fn pool<P: rocket::Phase>(
                    rocket: &rocket::Rocket<P>,
                ) -> Option<&rocket_sqlite_rw_pool::ConnectionPool<Self>> {
                    <rocket_sqlite_rw_pool::ConnectionPool<Self>>::get_pool(&rocket)
                }
            }

            pub struct [<$struct_name _Initializer>] {
                initializer: rocket_sqlite_rw_pool::PoolInitializerFn
            }

            impl [<$struct_name _Initializer>] {
                pub const fn new(initializer: rocket_sqlite_rw_pool::PoolInitializerFn) -> Self {
                    Self {
                        initializer
                    }
                }
            }

            impl From<&'static [<$struct_name _Initializer>]> for rocket_sqlite_rw_pool::PoolInitializer {
                fn from(initializer: &'static [<$struct_name _Initializer>]) -> Self {
                    Self {
                        initializer: initializer.initializer,
                    }
                }
            }

            rocket_sqlite_rw_pool::inventory::collect!(
                [<$struct_name _Initializer>]
            );
        }
    };
}

// TODO: We can probably unify the macros here

#[macro_export]
macro_rules! define_from_request_for_pool_holder {
    ($struct_name: ident) => {
        #[async_trait::async_trait]
        impl<'r, DB: 'static> rocket::request::FromRequest<'r> for $struct_name<'r, DB> {
            type Error = $crate::Error;

            #[inline]
            async fn from_request(
                request: &'r rocket::request::Request<'_>,
            ) -> rocket::request::Outcome<Self, Self::Error> {
                match request.rocket().state::<$crate::ConnectionPool<DB>>() {
                    Some(pool) => rocket::request::Outcome::Success($struct_name { pool }),
                    None => {
                        rocket::error!(
                            "Missing database fairing for `{}`",
                            std::any::type_name::<DB>()
                        );
                        rocket::request::Outcome::Error((
                            rocket::http::Status::InternalServerError,
                            Error::MissingDatabaseFairing(std::any::type_name::<DB>().to_owned()),
                        ))
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_sentinel_for_pool_holder {
    ($struct_name: ident) => {
        impl<'pool, DB: 'static> rocket::Sentinel for $struct_name<'pool, DB> {
            fn abort(rocket: &rocket::Rocket<rocket::Ignite>) -> bool {
                use rocket::yansi::Paint;
                if rocket.state::<$crate::ConnectionPool<DB>>().is_none() {
                    let conn = std::any::type_name::<DB>().bold();
                    let fairing_text = format!("{}::fairing()", conn);
                    let fairing = fairing_text.wrap().bold();
                    rocket::error!(
                        "requesting `{}` DB connection without attaching `{}`.",
                        conn,
                        fairing
                    );
                    rocket::info!("Attach `{}` to use database connection pooling.", fairing);
                    return true;
                }

                false
            }
        }
    };
}

#[macro_export]
macro_rules! define_from_request_for_gettable_connection {
    ($struct_name: ident, $getter: ident) => {
        #[async_trait::async_trait]
        impl<'r, DB: 'static> rocket::request::FromRequest<'r> for $struct_name<DB> {
            type Error = $crate::Error;

            #[inline]
            async fn from_request(
                request: &'r rocket::request::Request<'_>,
            ) -> rocket::request::Outcome<Self, Self::Error> {
                use rocket::outcome::IntoOutcome;

                match request.rocket().state::<$crate::ConnectionPool<DB>>() {
                    Some(pool) => pool
                        .$getter()
                        .await
                        .or_forward(rocket::http::Status::ServiceUnavailable),
                    None => {
                        rocket::error!(
                            "Missing database fairing for `{}`",
                            std::any::type_name::<DB>()
                        );
                        rocket::request::Outcome::Error((
                            rocket::http::Status::InternalServerError,
                            $crate::Error::MissingDatabaseFairing(
                                std::any::type_name::<DB>().to_owned(),
                            ),
                        ))
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_sentinel_for_gettable_connection {
    ($struct_name: ident) => {
        impl<DB: 'static> rocket::Sentinel for $struct_name<DB> {
            fn abort(rocket: &rocket::Rocket<rocket::Ignite>) -> bool {
                use rocket::yansi::Paint;
                if rocket.state::<$crate::ConnectionPool<DB>>().is_none() {
                    let conn = std::any::type_name::<DB>().bold();
                    let fairing_text = format!("{}::fairing()", conn);
                    let fairing = fairing_text.wrap().bold();
                    rocket::error!(
                        "requesting `{}` DB connection without attaching `{}`.",
                        conn,
                        fairing
                    );
                    rocket::info!("Attach `{}` to use database connection pooling.", fairing);
                    return true;
                }

                false
            }
        }
    };
}
