// Copyright (c) 2016 Brandon Thomas <bt@brand.io>

pub use response::DataResponse;
pub use response::Timestamp;
pub use std::time::Duration;
pub use url::ParseError;
pub use url::Url;
use hyper::client::Client;
use hyper::header::ContentType;
use hyper::header::Cookie;
use hyper::header::CookiePair;
use hyper::header::SetCookie;
use hyper::mime::Attr;
use hyper::mime::Mime;
use hyper::mime::SubLevel;
use hyper::mime::TopLevel;
use hyper::mime::Value;
use hyper::status::StatusCode;
use rustc_serialize::json;
use std::io::Read;
use url::UrlParser;

const DEFAULT_PORT : u32 = 8083u32;
const SESSION_COOKIE_NAME : &'static str = "ZWAYSession";

pub struct RazberryClient {
  base_url: Url,
  session_token: Option<String>,
  client: Client,
}

#[derive(Debug)]
pub enum RazberryError {
  ClientError,
  BadRequest,
  BadCredentials,
  ServerError,
}

#[derive(RustcDecodable, RustcEncodable)]
struct LoginRequest {
  /// Username.
  login: String,
  /// Password.
  password: String,
  /// Misc fields.
  default_ui: u8,
  /// Unknown parameter.
  form: bool,
  /// Unknown parameter.
  keepme: bool,
}

impl RazberryClient {
  /**
   * Construct a client from hostname, using the default port.
   */
  pub fn for_hostname(hostname: &str) -> Result<RazberryClient, ParseError> {
    RazberryClient::new(hostname, DEFAULT_PORT)
  }

  /**
   * Construct a client from hostname and port.
   */
  pub fn new(hostname: &str, port: u32) -> Result<RazberryClient, ParseError> {
    Url::parse(&format!("http://{}:{}", hostname, port)).map(|url| {
      RazberryClient {
        base_url: url,
        session_token: None,
        client: Client::new()
      }
    })
  }

  /**
   * Set the session for the cookie manually.
   */
  pub fn set_session_token(&mut self, credential: Option<String>) {
    self.session_token = credential;
  }

  /**
   * Get the session token.
   */
  pub fn get_session_token(&self) -> Option<String> {
    // TODO: Cleanup.
    self.session_token.as_ref().map(|s| s.to_string())
  }

  /**
   * Set HTTP client read timeout.
   */
  pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
    self.client.set_read_timeout(timeout)
  }

  /**
   * Set HTTP client write timeout.
   */
  pub fn set_write_timeout(&mut self, timeout: Option<Duration>) {
    self.client.set_write_timeout(timeout)
  }

  /**
   * Peform a login. If the attempt is successful, store the session token.
   */
  pub fn login(&mut self, username: &str, password: &str) -> Result<(), RazberryError> {
    let login_request = try!(json::encode(&LoginRequest {
      login: username.to_string(),
      password: password.to_string(),
      default_ui: 1,
      form: true,
      keepme: false,
    }).map_err(|_| RazberryError::ClientError));

    let url = try!(self.login_url());

    let result = try!(self.client.post(url)
        .body(&login_request)
        .header(ContentType(Mime(TopLevel::Application, SubLevel::Json,
                                     vec![(Attr::Charset, Value::Utf8)])))
        .send()
        .map_err(|_| RazberryError::ClientError));

    match result.status {
      StatusCode::Ok => {}, // Continue
      StatusCode::Unauthorized => { return Err(RazberryError::BadCredentials); },
      _ => { return Err(RazberryError::BadRequest); },
    }

    // Get the session cookie from the response.
    // TODO: Cleanup once `as_slice` becomes stable.
    let mut cookies = result.headers.get::<SetCookie>().unwrap().clone();
    let mut cookie = cookies.pop();
    while cookie.is_some() {
      {
        let c = cookie.unwrap();
        if &c.name == SESSION_COOKIE_NAME {
          self.session_token = Some(c.value);
          return Ok(());
        }
      }
      cookie = cookies.pop();
    }

    Err(RazberryError::ServerError)
  }

  /**
   * Get a full data dump of the state of the Razberry server and all
   * of its associated devices.
   */
  pub fn get_data(&self) -> Result<DataResponse, RazberryError> {
    self.fetch_data(None)
  }

  /**
   * Get a partial data dump of the state changes to the Razberry
   * server and associated devices that occurred after the provided
   * timestamp.
   */
  pub fn get_data_after(&self, timestamp: i64) -> Result<DataResponse, RazberryError> {
    self.fetch_data(Some(timestamp))
  }

  /**
   * Fastest way to look up the server timestamp.
   * Calls the data endpoint with an invalid timestamp.
   */
  pub fn get_server_timestamp(&self) -> Result<DataResponse, RazberryError> {
    self.fetch_data(Some(20000000000))
  }

  /// Do lookup at the data endpoint.
  pub fn fetch_data(&self, timestamp: Option<i64>)
      -> Result<DataResponse, RazberryError> {
    let url = try!(self.data_url(timestamp));
    let cookie = CookiePair::new(SESSION_COOKIE_NAME.to_string(),
                             self.session_token.clone().unwrap_or("".to_string()));

    let mut result = try!(self.client.get(url)
        .header(Cookie(vec![cookie]))
        .send()
        .map_err(|_| RazberryError::ClientError));

    match result.status {
      StatusCode::Ok => {}, // Continue
      StatusCode::Unauthorized => { return Err(RazberryError::BadCredentials); },
      _ => { return Err(RazberryError::BadRequest); },
    }

    let mut body = String::new();
    try!(result.read_to_string(&mut body)
         .map_err(|_| RazberryError::ServerError));

    DataResponse::from_str(&body).map_err(|_| RazberryError::ClientError)
  }

  /// Generate a data URL.
  fn data_url(&self, timestamp: Option<i64>) -> Result<Url, RazberryError> {
    let path = match timestamp {
      None => "/ZWaveAPI/Data".to_string(),
      Some(t) => format!("/ZWaveAPI/Data/{}", t),
    };
    UrlParser::new().base_url(&self.base_url)
        .parse(&path)
        .map_err(|_| RazberryError::ClientError)
  }

  /// Generate login URL.
  fn login_url(&self) -> Result<Url, RazberryError> {
    UrlParser::new().base_url(&self.base_url)
        .parse("/ZAutomation/api/v1/login")
        .map_err(|_| RazberryError::ClientError)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // TODO: More testing.

  #[test]
  fn client_with_hostname() {
    assert!(RazberryClient::for_hostname("localhost").is_ok())
  }

  #[test]
  fn client_with_hostname_and_port() {
    assert!(RazberryClient::new("localhost", 1234u32).is_ok())
  }
}

