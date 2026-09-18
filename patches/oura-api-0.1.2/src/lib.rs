//! # Oura API Client
//!
//! This crate provides a client for the [Oura V2 REST API](https://cloud.ouraring.com/v2/docs).
//!
//! Note that this client does not support the Oura V1 REST API.
//!
//! ## Usage
//! ```no_run
//! use oura_api::{OuraClient, DateQuery};
//!
//! // token is the personal access token for the Oura API
//! let token = std::env::var("OURA_PERSONAL_ACCESS_TOKEN").unwrap();
//! let client = OuraClient::new(&token);
//!
//! let august_date_query = DateQuery::builder().start_date("2023-08-01").end_date("2023-08-31").build();
//! let august_daily_sleep = client.list_daily_sleep(august_date_query).unwrap();
//! ```

pub mod models;

use paste::paste;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::models::*;

const API_BASE_URL: &str = "https://api.ouraring.com/v2/usercollection";

/// Query parameters for endpoints that accept a date range.
#[derive(Serialize, TypedBuilder)]
pub struct DateQuery<'a> {
    #[builder(default = None, setter(strip_option))]
    start_date: Option<&'a str>,
    #[builder(default = None, setter(strip_option))]
    end_date: Option<&'a str>,
    #[builder(default = None, setter(strip_option))]
    next_token: Option<&'a str>,
}

/// Query parameters for endpoints that accept a datetime range.
#[derive(Serialize, TypedBuilder)]
pub struct DatetimeQuery<'a> {
    #[builder(default = None, setter(strip_option))]
    start_datetime: Option<&'a str>,
    #[builder(default = None, setter(strip_option))]
    end_datetime: Option<&'a str>,
    #[builder(default = None, setter(strip_option))]
    next_token: Option<&'a str>,
}

/// Response from endpoints that return a list of items.
#[derive(Deserialize, Debug, PartialEq)]
pub struct ListResponse<T> {
    /// The list of items returned by the endpoint.
    pub data: Vec<T>,
    /// The optional token to use to retrieve the next page of results.
    pub next_token: Option<String>,
}

macro_rules! generic_endpoint {
    ($(#[$m:meta])*, $name: ident, $type: ty, $path: literal) => {
        $(#[$m])*
        pub fn $name(&self) -> Result<$type, reqwest::Error> {
            let url = format!("{}/{}", &self.base_url, $path);
            let response = self
                .client
                .get(&url)
                .bearer_auth(&self.token)
                .send()?
                .error_for_status()?
                .json::<$type>()?;
            Ok(response)
        }
    };
}

macro_rules! list_endpoint {
    ($(#[$m:meta])*, $name: ident, $type: ty, $path: literal, $query: ty) => {
        $(#[$m])*
        pub fn $name(&self, query: $query) -> Result<ListResponse<$type>, reqwest::Error> {
            let url = format!("{}/{}", &self.base_url, $path);
            let response = self
                .client
                .get(&url)
                .bearer_auth(&self.token)
                .query(&query)
                .send()?
                .error_for_status()?
                .json::<ListResponse<$type>>()?;
            Ok(response)
        }
    };
}

macro_rules! get_endpoint {
    ($(#[$m:meta])*, $name: ident, $type: ty, $path: literal) => {
        $(#[$m])*
        pub fn $name(&self, id: &str) -> Result<$type, reqwest::Error> {
            let url = format!("{}/{}/{}", &self.base_url, $path, id);
            let response = self
                .client
                .get(&url)
                .bearer_auth(&self.token)
                .send()?
                .error_for_status()?
                .json::<$type>()?;
            Ok(response)
        }
    };
}

macro_rules! endpoint_set {
    ($name: ident, $type: ty, $path: literal, $params: ty) => {
        paste! {
            get_endpoint!(#[doc = "Gets a single [" $type "] item by id."], [<get_ $name>], $type, $path);
            list_endpoint!(#[doc = "Returns a [ListResponse] of [" $type "] items based on the supplied query."], [<list_ $name>], $type, $path, $params);
        }
    };
}

/// The Oura API client.
///
/// This client is used to make requests to the Oura API.
pub struct OuraClient<'a> {
    token: &'a str,
    base_url: &'a str,
    client: Client,
}

impl<'a> OuraClient<'a> {
    /// Creates a new OuraClient from a personal access token.
    pub fn new(token: &'a str) -> Self {
        let client = Client::new();
        Self {
            token,
            client,
            base_url: API_BASE_URL,
        }
    }

    /// Creates a new OuraClient from a personal access token and a base URL.
    ///
    /// *Note:* This is only useful for testing against a mock server.
    pub fn build_with_base_url(token: &'a str, base_url: &'a str) -> Self {
        let client = Client::new();
        Self {
            token,
            client,
            base_url,
        }
    }

    endpoint_set!(daily_activity, DailyActivity, "daily_activity", DateQuery);

    endpoint_set!(
        daily_readiness,
        DailyReadiness,
        "daily_readiness",
        DateQuery
    );

    endpoint_set!(daily_sleep, DailySleep, "daily_sleep", DateQuery);

    endpoint_set!(daily_spo2, DailySpO2, "daily_spo2", DateQuery);

    list_endpoint!(#[doc = "Returns a [ListResponse] of [HeartRate] items based on the supplied query."], list_heart_rate, HeartRate, "heartrate", DatetimeQuery);

    generic_endpoint!(#[doc = "Returns a [PersonalInfo] based on the caller."], get_personal_info, PersonalInfo, "personal_info");

    endpoint_set!(
        rest_mode_period,
        RestModePeriod,
        "rest_mode_period",
        DateQuery
    );

    endpoint_set!(
        ring_configuration,
        RingConfiguration,
        "ring_configuration",
        DateQuery
    );

    endpoint_set!(session, Session, "session", DateQuery);

    endpoint_set!(sleep, Sleep, "sleep", DateQuery);

    endpoint_set!(sleep_time, SleepTime, "sleep_time", DateQuery);

    endpoint_set!(tag, Tag, "tag", DateQuery);

    endpoint_set!(workout, Workout, "workout", DateQuery);

    endpoint_set!(tag_v2, TagV2, "tag/v2", DateQuery);
}
