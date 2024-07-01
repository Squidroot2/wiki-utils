use std::error::Error;
use std::sync::{Arc, RwLock, PoisonError};
use std::fmt;

use futures::future::join_all;
use flurry::HashSet;
use flurry::HashMap;
use tokio::task::JoinError;
use log::error;


use crate::article::{Article, ArticleError};
use crate::client::AsyncClient;
use crate::client::ClientError;
use crate::url::decode_url_str;

type LayerRef = Arc<HashSet<String>>;
type LayerGroupRef = Arc<RwLock<Vec<LayerRef>>>;
type RedirectMapRef = Arc<HashMap<String, String>>;

const CLIENT_LIMIT: usize = 32;

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

    pub fn from_article(first_article: &Article) -> Result<Self, ArticleError> {
        let layer_zero: LayerRef = Self::layer_zero(first_article.get_endpoint().to_string());

        let mut links = first_article.get_article_links()?;

        let layer_one = HashSet::with_capacity(links.len());
        let guard = layer_one.guard();
        for link in links.drain() {
            layer_one.insert(link, &guard);
        }
        drop(guard);
        let layer_one = Arc::new(layer_one);

        let layers: Vec<LayerRef> = vec![
            layer_zero, layer_one
        ];

        let layers = Arc::new(RwLock::new(layers));

        Ok(LinkCalculator {
            layers,
            known_redirects: Arc::new(HashMap::new()),
        })
    }

    pub async fn compute_next_async(&mut self) -> Result<(), LinkCalcError> {
        let last_layer = self.get_last_layer()?;
        let this_layer = LayerRef::new(HashSet::new());

        let rounds = last_layer.len() / CLIENT_LIMIT + 1;
        let mut handles_all = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            handles_all.push(Vec::with_capacity(CLIENT_LIMIT))
        }
        let guard = last_layer.guard();

        let link_iter = last_layer.iter(&guard);

        for (index, link_ref) in link_iter.enumerate() {
            let link = link_ref.clone();
            let this_layer_clone = this_layer.clone();
            let known_redirects_clone = self.known_redirects.clone();
            let previous_layers_clone = self.layers.clone();

            let handle = tokio::spawn(async move  {
                Self::store_article_links(link, this_layer_clone, known_redirects_clone, previous_layers_clone).await
            });
            handles_all.get_mut(index / CLIENT_LIMIT).ok_or(LinkCalcError::HandleBoundsError)?.push(handle);
        }
        let mut results = Vec::with_capacity(last_layer.len());
        for handle_round in handles_all {
            let round_results = join_all(handle_round).await;
            results.extend(round_results);
        }

        let mut new_redirects = Vec::new();
        for result in results {
            match result {
                Ok(Ok(thread_new_redirects)) => {
                    new_redirects.extend(thread_new_redirects);
                },
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
    async fn store_article_links(link: String, this_layer: LayerRef, known_redirects: RedirectMapRef, previous_layers: LayerGroupRef) -> Result<Vec<(String, String)>, LinkCalcError> {
        let client = AsyncClient::new();
        let mut new_redirects = Vec::new();
        let neighbor_article = client.get_article(&link).await?;
        if link.ne(neighbor_article.get_endpoint()) {
            // We were redericted. Stores this
            new_redirects.push((link.to_string(), neighbor_article.get_endpoint().to_string()));
            let guard = known_redirects.guard();
            known_redirects.insert(link.to_string(), neighbor_article.get_endpoint().to_string(), &guard);
        }

        for neighbor_link in neighbor_article.get_article_links()? {
                if Self::find_in_previous_layer(previous_layers.clone(), known_redirects.clone(), &neighbor_link)?.is_none() {
                    let guard = this_layer.guard();
                    this_layer.insert(neighbor_link, &guard);
                }
            }
        Ok(new_redirects)
    }

    // Replace redirects
    fn normalize_layer(last_layer: LayerRef, new_redirects: Vec<(String, String)>) {
        let guard = last_layer.guard();
        for (link, target) in new_redirects {
            last_layer.remove(&link, &guard);
            last_layer.insert(target, &guard);
        }
    }

    fn find_in_previous_layer(previous_layers: LayerGroupRef, known_redirects: RedirectMapRef, endpoint: &str) -> Result<Option<usize>, LinkCalcError> {
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
        for layer in self.layers.read().unwrap().iter() {
            let guard = layer.guard();
            for endpoint in layer.iter(&guard) {
                match decode_url_str(endpoint) {
                    Ok(decoded) => {
                        writeln!(f, "{}", decoded)?;
                    },
                    Err(e) => {
                        error!("Failed to parse '{}'; Reason: {}", endpoint, e);
                        writeln!(f, "{}", endpoint)?;
                    }
                };

            }
            writeln!(f, "------------------")?;
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
    HandleBoundsError
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
    fn from(e: JoinError) ->LinkCalcError {
        LinkCalcError::JoinError(e)
    }
}

impl<T> From<PoisonError<T>> for LinkCalcError {
    fn from(_: PoisonError<T>) -> LinkCalcError {
        LinkCalcError::LockError
    }
}


impl Error for LinkCalcError {

}
