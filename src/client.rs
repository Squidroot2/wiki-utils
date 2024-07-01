use std::error::Error;
use std::fmt;

use reqwest::Client;
use scraper::Html;

use log::debug;

use crate::article::Article;

const BASE_URL: &str = "https://en.wikipedia.org/wiki/";
const RANDOM_ARTICLE_ENDPOINT: &str = "Special:Random";

#[derive(Default)]
pub struct AsyncClient {
    client: Client,
}

impl AsyncClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get_article(&self, article_name: &str) -> Result<Article, ClientError> {
        let mut url = String::from(BASE_URL);
        url.push_str(article_name);
        debug!("Sending request to {}", url);

        let response = self.client.get(&url).send().await?;

        let final_url = response.url().as_str();
        let final_endpoint = final_url.strip_prefix(BASE_URL).ok_or_else(|| ClientError::redirect(final_url.to_string()))?.to_owned();

        let response_text = response.text().await?;
        let html = Html::parse_document(&response_text);

        let article = Article::new(final_endpoint, html);
        Ok(article)
    }

    pub async fn get_random_article(&self) -> Result<Article, ClientError> {
        self.get_article(RANDOM_ARTICLE_ENDPOINT).await
    }
}

#[derive(Debug)]
pub enum ClientError {
    RequestError(reqwest::Error),
    RedirectError(String),
}

impl ClientError {
    fn redirect(url: String) -> ClientError {
        ClientError::RedirectError(url)
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO
        write!(f, "ClientError")
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> ClientError {
        ClientError::RequestError(e)
    }
}

impl Error for ClientError {

}

