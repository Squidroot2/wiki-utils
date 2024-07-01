use std::error::Error;
use std::collections::HashSet;
use std::fmt;

use once_cell::sync::Lazy;
use scraper::{ selectable::Selectable, ElementRef, Html, Selector};

static ARTICLE_BODY_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("#mw-content-text").unwrap()
});
static LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("a").unwrap()
});
static HEADING_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("#firstHeading span").unwrap()
});

pub struct Article {
    endpoint: String,
    html: Html,
}

impl Article {
    pub fn new(endpoint: String, html: Html) -> Self {
        Article {
            endpoint,
            html,
        }
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
                    _ => continue
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
        let body_parent = self.html.select(&ARTICLE_BODY_SELECTOR).next().ok_or(ArticleError::MissingBodyParent)?;
        let body = body_parent.first_child().ok_or(ArticleError::MissingBody)?;
        ElementRef::wrap(body).ok_or(ArticleError::ElementError)

    }

    pub fn get_article_links(&self) -> Result<HashSet<String>, ArticleError> {
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

#[derive(Debug)]
pub enum ArticleError {
    MissingBodyParent,
    MissingBody,
    MissingHeading,
    ElementError,
}

impl fmt::Display for ArticleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO
        write!(f, "ArticleError")
    }
}

impl Error for ArticleError {

}
