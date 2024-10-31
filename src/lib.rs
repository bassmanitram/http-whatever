//!
//! A Thread-safe version of [`snafu::Whatever`], which also allows for structured message
//! strings giving HTTP status code and application domain qualifiers, and allows
//! an Error to be turned into an [`http::Response`].
//! 
//! I fully admit that this flies in the face of "type-oriented" error handling, but
//! I really do feel that is overkill for most HTTP applications where one error (or 
//! one error chain) is the most you will get out of any request/response cycle, and
//! the goals are simply:
//! 
//! * Tell the user what went wrong with a standard HTTP status and message, and
//! * Log the error (chain) for further investigation if necessary
//! 
//! To that end, this allows you to use the "whatever..." context features from
//! [`snafu`] while still categorizing your errors and avoiding the boilerplate 
//! of creating error HTTP responses from those errors.
//! 
//! The message string is comprised of three colon-separated fields, with the first
//! two being optional:
//! 
//! * The HTTP status code - the default is `500`
//! * An arbitrary string denoting the 'domain' of the application that emitted the error.
//!   The significance of this is application-specific and no formatting rules are enforced
//!   for it (except that it cannot contain a colon). The default is "unknown", which is applied
//!   when the field is missing or when it contains the empty string.
//! * The message
//! 
//! # Examples
//! 
//! ## Basic use ala snafu::Whatever.
//! 
//! ```
//! use http_whatever::prelude::*;
//! fn parse_uint(uint_as_str: &str) -> HttpResult<usize> {
//!     uint_as_str.parse::<usize>().whatever_context("400:RequestContent:Bad value")
//! }
//! ```
//!
//! ## Using the macro
//! ```
//! use http_whatever::prelude::*;
//! fn parse_uint(uint_as_str: &str) -> HttpResult<usize> {
//!     uint_as_str.parse().whatever_context(http_err!(400,uint_as_str,"Bad input"))
//! }
//! ```
//! 
use core::fmt::Debug;
use std::error::Error;

use http::{Response, StatusCode, header::CONTENT_TYPE};
use snafu::{
    Snafu,
    Backtrace, whatever,
};

pub type HttpResult<A> = std::result::Result<A, HttpWhatever>;

///
/// A macro to help format the standard message strings used by this
/// error type.
/// 
/// http_err!(status<default 500>,domain<default "unknown">,msg)
///
#[macro_export]
macro_rules! http_err {
    ($s:expr,$d:expr,$e:expr) => {format!("{}:{}:{}",$s,$d,$e)};
    ($d:expr,$e:expr) => {format!("500:{}:{}",$d,$e)};
    ($e:expr) => {format!("500:unknown:{}",$e)};
}

///
/// An almost-drop-in replacement for [`snafu::Whatever`] with the following benefits:
/// 
/// * Conforms to the async magic incantation `Send + Sync + 'static` and so is thread-safe
///   and async-safe
/// * Can be transformed into an [http::Response] using information from the error to complete
///   the response
/// * A public `new` constructor that facilitates better ergonomics in certain error situations.
/// * A public `parts` method to retrieve the three parts of the error.
/// 
/// Otherwise it is exactly the same as [`snafu::Whatever`] and can be used in exactly the same
/// way.
/// 
/// (_almost-drop-in_ because, obviously, you have to use `HttpWhatever` as your error type).
/// 
#[derive(Debug, Snafu)]
#[snafu(whatever)]
#[snafu(display("{}", self.display()))]
pub struct HttpWhatever {
    #[snafu(source(from(Box<dyn std::error::Error + Send + Sync>, Some)))]
    #[snafu(provide(false))]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    message: String,
    backtrace: Backtrace,
}

impl HttpWhatever {
    ///
    /// Return the three parts of the message as a 3-element tuple.
    /// 
    /// The three parts are
    /// 
    /// * The `message` as a string slice
    /// * the `domain` as a string slice
    /// * the HTTP status code as a [`http::StatusCode`]
    /// 
    /// This method is useful if you wish to construct a customized response
    /// from the error, but still want the categorization that this error type
    /// allows.
    /// 
    pub fn parts(&self) -> (&str,&str,StatusCode) {
        let parts: Vec<&str> = self.message.splitn(3,':').collect::<Vec<&str>>();
        let mut idx = parts.len();

        let message = if idx == 0 {"<unknown>"} else {idx -= 1; parts[idx]};
        let domain = if idx == 0 {"Internal"} else {idx -= 1; parts[idx]};
        let status_code = if idx == 0 {StatusCode::INTERNAL_SERVER_ERROR} else {StatusCode::from_bytes(parts[idx-1].as_bytes()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)};

        (message, domain, status_code)
    }

    fn display(&self) -> String {
        let parts = self.parts();
        format!("{}: (Domain: {}, HTTP status: {})", parts.0, parts.1, parts.2)
    }

    ///
    /// Return a String that provides the `to_string()` output of this error and all nested sources.
    /// 
    pub fn details(&self) -> String {
        let mut s = self.to_string();
        let mut source = self.source();
        while let Some(e) = source {
            s.push_str(&format!("\n[{}]",e));
            source = e.source();
        }
        s
    }

    ///
    /// Return an [`http::Response<B>`] representation of the error, with
    /// a body generated from the `default` method of the generic body type.
    /// 
    pub fn as_http_response<B>(&self) -> Response<B> 
        where
            B: Default
    {
        let parts = self.parts();
        Response::builder().status(parts.2).body(B::default()).unwrap()
    }

    ///
    /// Return an [`http::Response<B>`] representation of the error, with
    /// a string body generated from the `into` method of the generic body 
    /// type.
    /// 
    /// The string in the response body will be of the format
    /// 
    /// `<message> (application domain: <domain>)`
    /// 
    /// The `content-type` header of the response will be `text/plain`.
    /// 
    pub fn as_http_string_response<B>(&self) -> Response<B> 
        where
            B: From<String>
    {
        let parts = self.parts();
        let body_str = format!("{} (application domain: {})", parts.0, parts.1);
        let body: B = body_str.into();
        Response::builder()
            .status(parts.2)
            .header(CONTENT_TYPE, "text/plain")
            .body(body)
            .unwrap()
    }

    ///
    /// Return an [`http::Response<B>`] representation of the error, with
    /// a JSON body generated from the `into` method.
    /// 
    /// The string in the response body will be of the format
    /// 
    /// `{"message":"<message>","domain":"<domain>"}`
    /// 
    /// The `content-type` header of the response will be `application/json`.
    /// 
    pub fn as_http_json_response<B>(&self) -> Response<B> 
        where
            B: From<String>
    {
        let parts = self.parts();
        let body_str = format!("{{\"message\":\"{}\",\"domain\":\"{}\"}}", parts.0, parts.1);
        let body: B = body_str.into();
        Response::builder()
            .status(parts.2)
            .header(CONTENT_TYPE, "application/json")
            .body(body)
            .unwrap()
    }

    ///
    /// Create a new `HttpWhatever` from the input string.
    /// 
    /// The input string should conform to the structure documented in the
    /// crate documentation.
    /// 
    pub fn new(message: impl std::fmt::Display) -> Self{
        let err_gen = |message|  -> HttpResult<()> {
            whatever!("{}",message)
        };
        err_gen(message).unwrap_err()
    }
}

///
///  A prelude of the main items required to use this type effectively.
/// 
/// This includes the important items from the [`snafu`] prelude, so _you_ do not
/// have to include the [`snafu`] prelude.
/// 
pub mod prelude {
    pub use snafu::{ensure, OptionExt as _, ResultExt as _};
    pub use snafu::{ensure_whatever, whatever};
    pub use crate::HttpWhatever;
    pub use crate::http_err;
    pub use crate::HttpResult;
}

#[cfg(test)]
mod tests {
    use std::num::ParseIntError;
    use crate::prelude::*;
    use http::{StatusCode, Response, header::CONTENT_TYPE};

    fn parse_usize(strint: &str) -> Result<usize, ParseIntError> {
        strint.parse()
    }

    #[test]
    fn basic_test() {
        let result: HttpWhatever = parse_usize("certainly not a usize").whatever_context("400:Input:That was NOT a usize!").unwrap_err();

        let parts = result.parts();
        assert_eq!(parts.0, "That was NOT a usize!");
        assert_eq!(parts.1, "Input");
        assert_eq!(parts.2, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn basic_details() {
        let result: HttpWhatever = parse_usize("certainly not a usize").whatever_context("400:Input:That was NOT a usize!").unwrap_err();

        assert_eq!(result.details(), "That was NOT a usize!: (Domain: Input, HTTP status: 400 Bad Request)\n[invalid digit found in string]");
    }

    #[test]
    fn test_macro() {
        let result: HttpWhatever = parse_usize("certainly not a usize").whatever_context(http_err!(400,"Input","That was NOT a usize!")).unwrap_err();

        let parts = result.parts();
        assert_eq!(parts.0, "That was NOT a usize!");
        assert_eq!(parts.1, "Input");
        assert_eq!(parts.2, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_new() {
        let result: HttpWhatever = HttpWhatever::new(&http_err!(403,"Input","That was NOT a usize!"));

        let parts = result.parts();
        assert_eq!(parts.0, "That was NOT a usize!");
        assert_eq!(parts.1, "Input");
        assert_eq!(parts.2, StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_response() {
        let result: HttpWhatever = HttpWhatever::new(&http_err!(403,"Input","That was NOT a usize!"));
        let http1: Response<String> = result.as_http_response();
        let http2: Response<String> = result.as_http_string_response();
        let http3: Response<String> = result.as_http_json_response();

        assert_eq!(http1.body(), "");
        assert_eq!(http1.status(), StatusCode::FORBIDDEN);
        assert_eq!(http2.body(), "That was NOT a usize! (application domain: Input)");
        assert_eq!(http2.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap(), "text/plain");
        assert_eq!(http3.body(), "{\"message\":\"That was NOT a usize!\",\"domain\":\"Input\"}");
        assert_eq!(http3.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap(), "application/json");
    }
}