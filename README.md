# http-whatever

![CI](https://github.com/bassmanitram/http-whatever/actions/workflows/rust.yml/badge.svg)

A Thread-safe version of [`snafu::Whatever`](https://github.com/shepmaster/snafu), 
which also allows for structured message strings giving HTTP status code and application 
domain qualifiers, and allows an Error to be turned into an [`http::Response`](https://docs.rs/http/latest/http/)`.

I fully admit that this flies in the face of "type-oriented" error handling, but
I really do feel that that is overkill for most HTTP applications where one error 
(or one error chain) is the most you will get out of any request/response cycle, and
the goals are simply:

a. Tell the user what went wrong with a standard HTTP status and message, and
b. Log the error (chain) for further investigation if necessary

To that end, this allows you to use the "whatever..." context features from
[`snafu`](https://github.com/shepmaster/snafu) while still categorizing your errors and avoiding the boilerplate 
of creating error HTTP responses from those errors.

# Examples

## Basic use as a drop-in for [`snafu::Whatever`].

```
use http_whatever::prelude::*;
fn parse_uint(uint_as_str: &str) -> Result<usize, HttpWhatever> {
    uint_as_str.parse().whatever_context("400:RequestContent:Bad value")?
}
```
## Using the macro
```
use http_whatever::prelude::*;
fn parse_uint(uint_as_str: &str) -> Result<usize, HttpWhatever> {
    uint_as_str.parse().whatever_context(http_err!(400,uint_as_str,"Bad input"))?
}
```

