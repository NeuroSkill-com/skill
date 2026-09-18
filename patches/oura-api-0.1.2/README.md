# rust-oura-api

A client for the Oura V2 REST API, written in Rust.

Documentation for the Oura V2 API is available [here](https://cloud.ouraring.com/v2/docs).

The client relies on [reqwest](https://docs.rs/reqwest/latest/reqwest/) for the underlying HTTP client and [serde](https://serde.rs) for serialization and deserialization.

## Features

Supports fetching following items from the Oura API and deserializing them into Rust structs:
-   Daily Activity
-   Daily Readiness
-   Daily Sleep
-   Daily SpO2
-   Heart Rate
-   Personal Info
-   Rest Mode Period
-   Ring Configuration
-   Session
-   Sleep
-   Sleep Time
-   Tag
-   Workout
-   TagV2

## Example Usage

```rust
use oura_api::{OuraClient, DateQuery};

// token is the personal access token for the Oura API
let token = std::env::var("OURA_PERSONAL_ACCESS_TOKEN").unwrap();
let client = OuraClient::new(&token);

let august_date_query = DateQuery::builder().start_date("2023-08-01").end_date("2023-08-31").build();
let august_daily_sleep = client.list_daily_sleep(august_date_query).unwrap();
```

For instructions on how to generate a personal access token, see the [Oura docs](https://cloud.ouraring.com/docs/authentication#personal-access-tokens).
