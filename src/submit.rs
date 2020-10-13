// Copyright 2018 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Contains functions which assist with batch submission to a REST API

use crate::error::CliError;

use futures::Stream;
use futures::{future, Future};
use hyper::header::{ContentLength, ContentType};
use hyper::{Client, Method, Request, Uri};
use protobuf::Message;
use sawtooth_sdk::messages::batch::BatchList;
use serde_derive::Deserialize;

#[derive(Deserialize, Debug)]
struct Link {
    link: String,
}

#[derive(Deserialize, Debug)]
pub struct StatusData {
    pub data: Vec<Status>,
    pub link: String,
}

#[derive(Deserialize, Debug)]
pub struct Status {
    // Batch id
    pub id: String,
    pub invalid_transactions: Vec<InvalidTransactions>,
    pub status: String,
}

#[derive(Deserialize, Debug)]
pub struct InvalidTransactions {
    // Transactions id
    pub id: String,
    pub message: String,
}

pub fn submit_batch_list(url: &str, batch_list: &BatchList) -> Result<String, CliError> {
    let post_url = String::from(url) + "/api/batches";
    let hyper_uri = post_url.parse::<Uri>()?;

    match hyper_uri.scheme() {
        Some(scheme) => {
            if scheme != "http" {
                return Err(CliError::UserError(format!(
                    "Unsupported scheme ({}) in URL: {}",
                    scheme, url
                )));
            }
        }
        None => {
            return Err(CliError::UserError(format!("No scheme in URL: {}", url)));
        }
    }

    let mut core = tokio_core::reactor::Core::new()?;
    let handle = core.handle();
    let client = Client::configure().build(&handle);

    let bytes = batch_list.write_to_bytes()?;

    let mut req = Request::new(Method::Post, hyper_uri);
    req.headers_mut().set(ContentType::octet_stream());
    req.headers_mut().set(ContentLength(bytes.len() as u64));
    req.set_body(bytes);

    let work = client.request(req).and_then(|res| {
        res.body()
            .concat2()
            .and_then(move |chunks| future::ok(serde_json::from_slice::<Link>(&chunks).unwrap()))
    });

    let batch_link = core.run(work)?;
    Ok(batch_link.link)
}

pub fn wait_for_status(base_url: &str, batch_status_link: &str) -> Result<StatusData, CliError> {
    let link = format!("{}/api{}{}", base_url, batch_status_link, "&wait=true");
    let req = Request::new(Method::Get, link.parse::<Uri>()?);

    // Create client
    let mut core = tokio_core::reactor::Core::new()?;
    let handle = core.handle();
    let client = Client::configure().build(&handle);

    let work = client.request(req).and_then(|res| {
        res.body().concat2().and_then(move |chunks| {
            future::ok(serde_json::from_slice::<StatusData>(&chunks).unwrap())
        })
    });

    let batch_status = core.run(work)?;
    Ok(batch_status)
}
