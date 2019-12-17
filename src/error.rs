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

//! Contains functions which assist with error management

use std;
use std::borrow::Borrow;
use std::error::Error as StdError;

use hyper;
use protobuf;
use sawtooth_sdk::signing;

#[derive(Debug)]
pub enum CliError {
    /// The user has provided invalid inputs; the string by this error
    /// is appropriate for display to the user without additional context
    UserError(String),
    IoError(std::io::Error),
    SigningError(signing::Error),
    ProtobufError(protobuf::ProtobufError),
    HyperError(hyper::Error),
    InvalidTransactionError(String),
    InvalidInputError(String),
}

impl StdError for CliError {
    fn description(&self) -> &str {
        match *self {
            CliError::UserError(ref s) => &s,
            CliError::IoError(ref err) => err.description(),
            CliError::SigningError(ref err) => err.description(),
            CliError::ProtobufError(ref err) => err.description(),
            CliError::HyperError(ref err) => err.description(),
            CliError::InvalidTransactionError(ref s) => &s,
            CliError::InvalidInputError(ref s) => &s,
        }
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match *self {
            CliError::UserError(ref _s) => None,
            CliError::IoError(ref err) => Some(err.borrow()),
            CliError::SigningError(ref err) => Some(err.borrow()),
            CliError::ProtobufError(ref err) => Some(err.borrow()),
            CliError::HyperError(ref err) => Some(err.borrow()),
            CliError::InvalidTransactionError(ref _s) => None,
            CliError::InvalidInputError(ref _s) => None,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            CliError::UserError(ref s) => write!(f, "Error: {}", s),
            CliError::IoError(ref err) => write!(f, "IoError: {}", err),
            CliError::SigningError(ref err) => write!(f, "SigningError: {}", err.description()),
            CliError::ProtobufError(ref err) => write!(f, "ProtobufError: {}", err.description()),
            CliError::HyperError(ref err) => write!(f, "HyperError: {}", err.description()),
            CliError::InvalidTransactionError(ref s) => write!(f, "InvalidTransactionError: {}", s),
            CliError::InvalidInputError(ref s) => write!(f, "InvalidInput: {}", s),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::IoError(e)
    }
}

impl From<protobuf::ProtobufError> for CliError {
    fn from(e: protobuf::ProtobufError) -> Self {
        CliError::ProtobufError(e)
    }
}

impl From<signing::Error> for CliError {
    fn from(e: signing::Error) -> Self {
        CliError::SigningError(e)
    }
}

impl From<hyper::Error> for CliError {
    fn from(e: hyper::Error) -> Self {
        CliError::HyperError(e)
    }
}

impl From<hyper::error::UriError> for CliError {
    fn from(err: hyper::error::UriError) -> Self {
        CliError::UserError(format!("Invalid URL: {}", err))
    }
}
