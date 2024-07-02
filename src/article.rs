use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use log::{error, warn};
use once_cell::sync::Lazy;
use scraper::{selectable::Selectable, ElementRef, Html, Selector};

const ARTICLE_BODY_CSS: &str = "#mw-content-text";
const HEADING_CSS: &str = "#firstHeading span";

static ARTICLE_BODY_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(ARTICLE_BODY_CSS).unwrap());
static HEADING_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(HEADING_CSS).unwrap());
static LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a[href^='/wiki/'").unwrap());

pub struct Article {
    endpoint: String,
    html: Html,
}

impl Article {
    pub fn new(endpoint: String, html: Html) -> Self {
        let errors = html.errors.join(";");
        if !errors.is_empty() {
            warn!("Instantiating Article '{}' with errors: {}", endpoint, errors);
        }
        Article { endpoint, html }
    }

    pub fn get_endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn get_lead_string(&self) -> Result<String, ArticleError> {
        let inner_nodes = self.get_article_body()?.children();
        let mut lead_paragraphs = Vec::new();
        for node in inner_nodes {
            if let Some(element) = node.value().as_element() {
                match element.name() {
                    "p" => lead_paragraphs.push(ElementRef::wrap(node).ok_or(ArticleError::ElementError)?),
                    "h2" => break,
                    _ => continue,
                }
            }
        }
        let mut lead_string = String::new();
        for paragraph in lead_paragraphs {
            lead_string.push_str(&paragraph.html());
        }
        Ok(lead_string)
    }

    pub fn get_article_body(&self) -> Result<ElementRef<'_>, ArticleError> {
        let body_parent = self
            .html
            .select(&ARTICLE_BODY_SELECTOR)
            .next()
            .ok_or(ArticleError::MissingBodyParent)?;
        let body = body_parent.first_child().ok_or(ArticleError::MissingBody)?;
        ElementRef::wrap(body).ok_or_else(|| {
            error!("Failed to wrap node '{:?}' as element", body);
            ArticleError::ElementError
        })
    }

    pub fn create_article_link_set(&self) -> Result<HashSet<String>, ArticleError> {
        let article_body = self.get_article_body()?;
        let links = article_body.select(&LINK_SELECTOR);
        let mut endpoints = HashSet::new();
        for link in links {
            if let Some(href) = link.value().attr("href") {
                if let Some(wiki_link) = href.strip_prefix("/wiki/") {
                    if !wiki_link.contains(':') {
                        let page_wiki_link = wiki_link.split('#').next().expect("Will always have one element in split");
                        endpoints.insert(page_wiki_link.to_owned());
                    }
                }
            }
        }

        Ok(endpoints)
    }

    pub fn get_article_title(&self) -> Result<String, ArticleError> {
        let heading_span = self.html.select(&HEADING_SELECTOR).next().ok_or(ArticleError::MissingHeading)?;
        Ok(heading_span.inner_html())
    }
}

impl<'this> Article {
    pub fn get_article_link_refs(&'this self) -> Result<HashSet<&'this str>, ArticleError> {
        let article_body = self.get_article_body()?;
        let links = article_body.select(&LINK_SELECTOR);
        let mut endpoints = HashSet::new();
        for link in links {
            if let Some(href) = link.value().attr("href") {
                if let Some(wiki_link) = href.strip_prefix("/wiki/") {
                    if !wiki_link.contains(':') {
                        let page_wiki_link = wiki_link.split('#').next().expect("Will always have one element in split");
                        endpoints.insert(page_wiki_link);
                    }
                }
            }
        }

        Ok(endpoints)
    }
}

#[derive(Debug)]
pub enum ArticleError {
    MissingBodyParent,
    MissingBody,
    MissingHeading,
    ElementError,
}

impl fmt::Display for ArticleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBodyParent => write!(f, "Cannot find element with css '{}'", ARTICLE_BODY_CSS),
            Self::MissingBody => write!(f, "Cannot find child of element with css '{}'", ARTICLE_BODY_CSS),
            Self::MissingHeading => write!(f, "Cannot find element with css '{}'", HEADING_CSS),
            Self::ElementError => write!(f, "Failed to convert node to element"),
        }
    }
}

impl Error for ArticleError {}
