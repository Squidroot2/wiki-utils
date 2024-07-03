use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::{Client, Response, StatusCode};
use scraper::Html;
use tokio::sync::AcquireError;
use tokio::sync::Semaphore;
use tokio::time;
use tokio::time::Duration;

use log::{debug, trace};

use crate::article::Article;

const BASE_URL: &str = "https://en.wikipedia.org/wiki/";
const RANDOM_ARTICLE_ENDPOINT: &str = "Special:Random";
const MAX_RETRIES: usize = 5;
const RETRY_INTERVAL: Duration = Duration::from_millis(2000);

static CONNECTION_PERMITS: Semaphore = Semaphore::const_new(100);

#[derive(Default)]
pub struct AsyncClient {
    client: Client,
    paused: AtomicBool,
}

impl AsyncClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_article(&self, article_name: &str) -> Result<Article, ClientError> {
        let mut url = String::from(BASE_URL);
        url.push_str(article_name);
        debug!("Sending request to {}", url);

        let response = self.get_request(&url).await?;

        let final_url = response.url().as_str();
        let final_endpoint = final_url.strip_prefix(BASE_URL).ok_or(ClientError::RedirectError)?.to_owned();

        let response_text = response.text().await?;
        trace!("Response from {}:\n{}", final_endpoint, response_text);
        let html = Html::parse_document(&response_text);

        let article = Article::new(final_endpoint, html);
        Ok(article)
    }

    pub async fn get_random_article(&self) -> Result<Article, ClientError> {
        self.get_article(RANDOM_ARTICLE_ENDPOINT).await
    }

    async fn get_request(&self, url: &str) -> Result<Response, ClientError> {
        let mut retries = 0;
        let mut last_try_result = Err(ClientError::Default);

        loop {
            if retries == MAX_RETRIES {
                break;
            }
            retries += 1;
            if self.paused.load(Ordering::SeqCst) {
                last_try_result = Err(ClientError::PausedOnOtherThread);
                debug!("GET '{}' Attempt {}: Paused on other thread", url, retries);
                time::sleep(RETRY_INTERVAL).await;
                continue;
            }
            let permit = CONNECTION_PERMITS.acquire().await?;
            let result = self.client.get(url).send().await?;
            drop(permit);

            last_try_result = result.error_for_status().map_err(|e| ClientError::from(e));
            match &last_try_result {
                Err(e) => {
                    debug!("GET '{}' Attempt {}: Failed with Error '{}'", url, retries, e);
                    if e.status_code().is_some_and(|code| code == StatusCode::NOT_FOUND) {
                        // Not going to bother trying again for 404 errors
                        break;
                    }
                    self.paused.store(true, Ordering::SeqCst);
                    time::sleep(RETRY_INTERVAL).await;
                    debug!("Resuming from pause");
                    self.paused.store(false, Ordering::SeqCst);
                }
                Ok(_) => {
                    trace!("GET '{}' Succeeded", url);
                    break;
                }
            };
        }

        last_try_result
    }
}

#[derive(Debug)]
pub enum ClientError {
    Default,
    RequestError(reqwest::Error),
    StatusCodeError(reqwest::StatusCode),
    RedirectError,
    SemaphoreAcquireError(AcquireError),
    PausedOnOtherThread,
}

impl ClientError {
    pub fn status_code(&self) -> Option<StatusCode> {
        match self {
            Self::StatusCodeError(code) => Some(*code),
            _ => None,
        }
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "Unspecified client error"),
            Self::RequestError(e) => write!(f, "Request Failed: {}", e),
            Self::StatusCodeError(code) => write!(f, "Request returned status code: {}", code),
            Self::RedirectError => write!(f, "Redirected to different site"),
            Self::SemaphoreAcquireError(e) => write!(f, "Failed to acquire Semaphore: {}", e),
            Self::PausedOnOtherThread => write!(f, "Other threads paused. Could not attempt request"),
        }
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> ClientError {
        match e.status() {
            Some(code) => Self::StatusCodeError(code),
            None => Self::RequestError(e),
        }
    }
}

impl From<AcquireError> for ClientError {
    fn from(e: AcquireError) -> Self {
        Self::SemaphoreAcquireError(e)
    }
}

impl Error for ClientError {}
