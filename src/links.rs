use std::error::Error;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock};

use flurry::HashMap;
use flurry::HashSet;
use futures::future::join_all;
use log::{debug, error, info};
use tokio::task::JoinError;

use crate::article::{Article, ArticleError};
use crate::client::AsyncClient;
use crate::client::ClientError;
use crate::url::decode_url_str;

type LayerRef = Arc<HashSet<String>>;
type LayerGroupRef = Arc<RwLock<Vec<LayerRef>>>;
type RedirectMapRef = Arc<HashMap<String, String>>;

#[derive(Debug)]
pub struct LinkCalculator {
    layers: LayerGroupRef,
    known_redirects: RedirectMapRef,
}

impl LinkCalculator {
    // Create First Layer containing start point
    fn layer_zero(start_point: String) -> LayerRef {
        let start: LayerRef = Arc::new(HashSet::with_capacity(1));
        let guard = start.guard();
        start.insert(start_point, &guard);
        drop(guard);
        start
    }

    pub fn new(start_point: String) -> Self {
        let mut layers: Vec<LayerRef> = Vec::new();

        let start = Self::layer_zero(start_point);

        // Add Layer to layers
        layers.push(start);
        let layers = Arc::new(RwLock::new(layers));
        LinkCalculator {
            layers,
            known_redirects: Arc::new(HashMap::new()),
        }
    }

    pub fn get_layer_count(&self) -> Result<usize, LinkCalcError> {
        Ok(self.layers.read()?.len())
    }

    pub fn from_article(first_article: &Article) -> Result<Self, ArticleError> {
        let layer_zero: LayerRef = Self::layer_zero(first_article.get_endpoint().to_string());

        //let mut links = first_article.create_article_link_set()?;
        let links = first_article.get_article_link_refs()?;

        let layer_one = HashSet::with_capacity(links.len());
        let guard = layer_one.guard();
        for link in links {
            layer_one.insert(link.to_owned(), &guard);
        }
        drop(guard);
        let layer_one = Arc::new(layer_one);

        let layers = Arc::new(RwLock::new(vec![layer_zero, layer_one]));

        Ok(LinkCalculator {
            layers,
            known_redirects: Arc::new(HashMap::new()),
        })
    }

    pub async fn compute_next_async(&mut self) -> Result<(), LinkCalcError> {
        info!("Calculating layer {}", self.get_layer_count()?);
        let client = Arc::new(AsyncClient::new());

        let last_layer = self.get_last_layer()?;
        let this_layer = LayerRef::new(HashSet::new());

        let mut handles = Vec::with_capacity(last_layer.len());
        let guard = last_layer.guard();

        let link_iter = last_layer.iter(&guard);

        for link_ref in link_iter {
            let link = link_ref.clone();
            let this_layer_clone = this_layer.clone();
            let known_redirects_clone = self.known_redirects.clone();
            let previous_layers_clone = self.layers.clone();
            let client_clone = client.clone();

            let handle = tokio::spawn(async move {
                Self::store_article_links(&client_clone, link, this_layer_clone, known_redirects_clone, previous_layers_clone).await
            });

            handles.push(handle);
        }
        let results = join_all(handles).await;

        let mut new_redirects = Vec::new();
        for result in results {
            match result {
                Ok(Ok(thread_new_redirects)) => {
                    new_redirects.extend(thread_new_redirects);
                }
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(e.into()),
            };
        }

        Self::normalize_layer(last_layer.clone(), new_redirects);
        self.layers.write()?.push(this_layer);

        Ok(())
    }

    pub async fn compute_layers_async(&mut self, count: usize) -> Result<(), LinkCalcError> {
        for _ in 0..count {
            self.compute_next_async().await?;
        }

        Ok(())
    }

    fn get_last_layer(&self) -> Result<LayerRef, LinkCalcError> {
        Ok(self.layers.read()?.last().ok_or(LinkCalcError::NotInitializedError)?.clone())
    }

    // Returns new article redirects
    async fn store_article_links(
        client: &AsyncClient,
        link: String,
        this_layer: LayerRef,
        known_redirects: RedirectMapRef,
        previous_layers: LayerGroupRef,
    ) -> Result<Option<(String, String)>, LinkCalcError> {
        let neighbor_article = client.get_article(&link).await?;

        let new_redirect = match link.eq(neighbor_article.get_endpoint()) {
            true => None,
            false => {
                info!("Found redirect: {} -> {}", link, neighbor_article.get_endpoint());
                let guard = known_redirects.guard();
                known_redirects.insert(link.to_string(), neighbor_article.get_endpoint().to_string(), &guard);
                Some((link.to_string(), neighbor_article.get_endpoint().to_string()))
            }
        };

        let neighbor_links = match neighbor_article.create_article_link_set() {
            Ok(links) => links,
            Err(e) => {
                error!("Failed to identify links for article '{}'; Reason: '{}'", link, e);
                return Err(LinkCalcError::from(e));
            }
        };

        for neighbor_link in neighbor_links {
            if Self::find_in_previous_layer(previous_layers.clone(), known_redirects.clone(), &neighbor_link)?.is_none() {
                let guard = this_layer.guard();
                this_layer.insert(neighbor_link, &guard);
            }
        }
        debug!("Finished storing links for endpoint {}", link);
        Ok(new_redirect)
    }

    // Replace redirects
    fn normalize_layer(last_layer: LayerRef, new_redirects: Vec<(String, String)>) {
        let guard = last_layer.guard();
        for (link, target) in new_redirects {
            last_layer.remove(&link, &guard);
            last_layer.insert(target, &guard);
        }
    }

    fn find_in_previous_layer(
        previous_layers: LayerGroupRef,
        known_redirects: RedirectMapRef,
        endpoint: &str,
    ) -> Result<Option<usize>, LinkCalcError> {
        let guard = known_redirects.guard();
        let real_endpoint = known_redirects.get(endpoint, &guard).map_or(endpoint, |s| s.as_str());
        for (layer_num, layer) in previous_layers.read()?.iter().enumerate() {
            let guard = layer.guard();
            if layer.contains(real_endpoint, &guard) {
                return Ok(Some(layer_num));
            }
        }
        Ok(None)
    }
}

impl fmt::Display for LinkCalculator {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let unlocked_layers = self.layers.read().map_err(|_| fmt::Error)?;

        let mut layer_iter = unlocked_layers.iter().enumerate();

        // First, Write the article name
        let (_0, layer_zero) = layer_iter.next().ok_or(fmt::Error)?;
        let guard = layer_zero.guard();
        let article_name = layer_zero.iter(&guard).next().map(|a| a.as_str()).unwrap_or("");
        match decode_url_str(article_name) {
            Ok(decoded) => {
                writeln!(f, "Article Name: {}", decoded)?;
            }
            Err(e) => {
                error!("Failed to parse '{}'; Reason: {}", article_name, e);
                writeln!(f, "Endpoint Name (unparseable): {}", article_name)?;
            }
        }

        while let Some((i, layer)) = layer_iter.next() {
            let guard = layer.guard();
            writeln!(f, "{}-Hop Neighbors ({}):", i, layer.len())?;
            for endpoint in layer.iter(&guard) {
                match decode_url_str(endpoint) {
                    Ok(decoded) => {
                        writeln!(f, "\t{}", decoded)?;
                    }
                    Err(e) => {
                        error!("Failed to parse '{}'; Reason: {}", endpoint, e);
                        writeln!(f, "\t{}", endpoint)?;
                    }
                };
            }
        }

        writeln!(f, "Known Redirects ({}):", self.known_redirects.len())?;

        let guard = self.known_redirects.guard();
        for (link, target) in self.known_redirects.iter(&guard) {
            writeln!(f, "\t{} -> {}", link, target)?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum LinkCalcError {
    ArticleError(ArticleError),
    ClientError(ClientError),
    LockError,
    NotInitializedError,
    JoinError(JoinError),
}

impl fmt::Display for LinkCalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO
        write!(f, "LinkCalcError")
    }
}

impl From<ArticleError> for LinkCalcError {
    fn from(e: ArticleError) -> LinkCalcError {
        LinkCalcError::ArticleError(e)
    }
}

impl From<ClientError> for LinkCalcError {
    fn from(e: ClientError) -> LinkCalcError {
        LinkCalcError::ClientError(e)
    }
}

impl From<JoinError> for LinkCalcError {
    fn from(e: JoinError) -> LinkCalcError {
        LinkCalcError::JoinError(e)
    }
}

impl<T> From<PoisonError<T>> for LinkCalcError {
    fn from(_: PoisonError<T>) -> LinkCalcError {
        LinkCalcError::LockError
    }
}

impl Error for LinkCalcError {}
