use rocket::{
    http::Status,
    request::{FromRequest, Outcome, Request},
};

use rocket_csrf_guard::CsrfCheckProof;

/// Enum representing different types of write authorizations.
/// Used for security checks and other related operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteAuthorization {
    /// Represents a request that has passed CSRF checks.
    /// Ideally, this should be tied to a request with a lifetime,
    /// but to avoid carrying around lifetimes everywhere, it's not.
    /// This might be revisited in the future.
    PassedCsrfChecks,

    /// Represents a background job that is not tied to a request.
    IPromiseThisIsABackgroundJobNotTiedToARequest,

    /// Represents a logging endpoint that is safe to write without protection.
    ThisIsALoggingEndpointAndSafeToWriteWithoutProtection,
}

impl std::convert::From<CsrfCheckProof> for WriteAuthorization {
    fn from(proof: CsrfCheckProof) -> Self {
        match proof {
            CsrfCheckProof::PassedCsrfChecks => Self::PassedCsrfChecks,
        }
    }
}

/// This allows a [`WriteAuthorization`] to be created from a [`Request`] iff it has passed CSRF checks.
/// By default, this considers the request as unauthorized.
/// Users, if desired, need to run CSRF checks *before* this one and populate the cache.
#[async_trait::async_trait]
impl<'r> FromRequest<'r> for WriteAuthorization {
    type Error = std::convert::Infallible;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let cached = request.local_cache(|| {
            // If not set, get from the [`CsrfCheckProof`]
            let proof: &Option<CsrfCheckProof> = request.local_cache(|| None);
            proof.as_ref().cloned().map_or_else(
                || Self::IPromiseThisIsABackgroundJobNotTiedToARequest,
                Self::from,
            )
        });

        if *cached == Self::IPromiseThisIsABackgroundJobNotTiedToARequest {
            return Outcome::Forward(Status::Ok);
        }
        Outcome::Success(cached.clone())
    }
}
