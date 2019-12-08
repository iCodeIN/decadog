use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::Hasher;

use log::{debug, error};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use reqwest::{Client as ReqwestClient, Method, RequestBuilder, Url, UrlError};
use serde::de::DeserializeOwned;
use serde_derive::{Deserialize, Serialize};

use crate::core::{Issue, Milestone, OrganisationMember, Repository};
use crate::error::Error;

/// Decadog client, used to abstract complex tasks over the Github API.
pub struct Client {
    id: u64,
    reqwest_client: ReqwestClient,
    headers: HeaderMap,
    base_url: Url,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Decadog client {}", self.id)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubClientErrorDetail {
    pub resource: String,
    pub field: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GithubClientErrorBody {
    pub message: String,
    pub errors: Option<Vec<GithubClientErrorDetail>>,
    pub documentation_url: Option<String>,
}

/// Send a HTTP request to Github, and return the resulting struct.
trait SendGithubExt {
    fn send_github<T>(self) -> Result<T, Error>
    where
        Self: Sized,
        T: DeserializeOwned;
}

impl SendGithubExt for RequestBuilder {
    fn send_github<T>(self) -> Result<T, Error>
    where
        Self: Sized,
        T: DeserializeOwned,
    {
        let mut response = self.send()?;
        let status = response.status();
        if status.is_success() {
            Ok(response.json()?)
        } else if status.is_client_error() {
            Err(Error::GithubClient {
                error: response.json()?,
                status,
            })
        } else {
            Err(Error::Api {
                description: "Unexpected response status code.".to_owned(),
                status,
            })
        }
    }
}

impl Client {
    /// Create a new client that can make requests to the Github API using token auth.
    pub fn new(url: &str, token: &str) -> Result<Client, Error> {
        // Create reqwest client to interact with APIs
        // TODO: should we pass in an external client here?
        let reqwest_client = reqwest::Client::new();

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("token {}", token)
                .parse()
                .map_err(|_| Error::Config {
                    description: "Invalid Github token for Authorization header.".to_owned(),
                })?,
        );

        let base_url = Url::parse(url).map_err(|_| Error::Config {
            description: format!("Invalid Github base url {}", url),
        })?;

        let mut hasher = DefaultHasher::new();
        hasher.write(url.as_bytes());
        hasher.write(token.as_bytes());
        let id = hasher.finish();

        Ok(Client {
            id,
            reqwest_client,
            headers,
            base_url,
        })
    }

    /// Returns a `request::RequestBuilder` authorized to the Github API.
    pub fn request(&self, method: Method, url: Url) -> Result<RequestBuilder, UrlError> {
        debug!("{} {}", method, url.as_str());
        Ok(self
            .reqwest_client
            .request(method, url)
            .headers(self.headers.clone()))
    }

    /// Get an issue by owner, repo name and issue number.
    pub fn get_issue(&self, owner: &str, repo: &str, issue_number: u32) -> Result<Issue, Error> {
        self.request(
            Method::GET,
            self.base_url
                .join(owner)?
                .join(repo)?
                .join("issues")?
                .join(&issue_number.to_string())?,
        )?
        .send_github()
    }

    /// Get a repository by owner and repo name.
    pub fn get_repository(&self, owner: &str, repo: &str) -> Result<Repository, Error> {
        self.request(Method::GET, self.base_url.join(owner)?.join(repo)?)?
            .send_github()
    }

    /// Get members by organisation.
    pub fn get_members(&self, organisation: &str) -> Result<Vec<OrganisationMember>, Error> {
        self.request(
            Method::GET,
            self.base_url
                .join("orgs")?
                .join(organisation)?
                .join("members")?,
        )?
        .send_github()
    }

    /// Get milestones by owner and repo name.
    pub fn get_milestones(&self, owner: &str, repo: &str) -> Result<Vec<Milestone>, Error> {
        self.request(
            Method::GET,
            self.base_url.join(owner)?.join(repo)?.join("milestones")?,
        )?
        .send_github()
    }

    /// Update issue.
    pub fn patch_issue(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u32,
        update: &IssueUpdate,
    ) -> Result<Issue, Error> {
        self.request(
            Method::PATCH,
            self.base_url
                .join(owner)?
                .join(repo)?
                .join("issues")?
                .join(&issue_number.to_string())?,
        )?
        .json(update)
        .send_github()
    }

    /// Search issues.
    pub fn search_issues(&self, query: &SearchIssues) -> Result<Vec<Issue>, Error> {
        let builder = self
            .request(Method::GET, self.base_url.join("search")?.join("issues")?)?
            .query(&query);

        let results: GithubSearchResults<Issue> = builder.send_github()?;
        if results.incomplete_results {
            // FIXME handle github pagination
            error!("Incomplete results recieved from Github Search API, this is bad");
        }
        Ok(results.items)
    }
}

/// Update an issue.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct IssueUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milestone: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignees: Option<Vec<String>>,
}

/// Request to search issues.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct SearchIssues {
    pub q: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

/// A response from the Github search API.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct GithubSearchResults<T> {
    pub incomplete_results: bool,
    pub items: Vec<T>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn invalid_github_token() {
        assert!(Client::new("https://api.mygithub.com/", "github_token").is_ok());
        match Client::new("https://api.mygithub.com/", "invalid header char -> \n").unwrap_err() {
            Error::Config { description } => assert_eq!(
                description,
                "Invalid Github token for Authorization header."
            ),
            _ => panic!("Unexpected error"),
        }
    }
}