use anyhow::{Context, Result};
use itertools::Itertools;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
};
use tokio::{io::AsyncWriteExt, task::JoinSet};
use tracing::info;

use crate::utils::download_util::{download_file, create_dir};

#[derive(Serialize, Deserialize, Debug)]
pub struct Assets {
    pub objects: HashMap<String, AssetInformation>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AssetInformation {
    pub hash: String,
    pub size: i64,
}

#[derive(Debug)]
pub struct AssetsDownload {
    assets: Assets,
    id: String,
    url: String,
}

impl AssetsDownload {
    pub async fn new(url: String, id: String) -> Result<Self> {
        Ok(Self {
            assets: Self::init(url.clone()).await?,
            id,
            url,
        })
    }

    async fn init(url: String) -> Result<Assets> {
        let data: Assets = Client::new()
            .get(url)
            .send()
            .await
            .context("failed to send get request")?
            .json()
            .await
            .context("failed to parse json")?;

        Ok(data)
    }

    pub async fn get_assets_json(&self, assets_dir: &Path) -> Result<()> {
        let filen = format!("{}.json", self.id);
        let path = assets_dir.join(filen);

        if let Some(path) = path.parent() {
            tokio::fs::create_dir_all(path).await?;
        }

        let body = Client::new().get(&self.url).send().await?.text().await?;

        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(body.as_bytes()).await?;

        Ok(())
    }

    pub async fn download_assets_chunked(&self, dir: &Path) -> anyhow::Result<()> {
        let assets_chunks = self.assets.objects.iter().chunks(100);

        // TODO: Implement retry pull for missing assets

        for asset in &assets_chunks {
            let assets: Vec<_> = asset.collect();

            let mut set = JoinSet::new();
            for (_k, v) in assets {
                let path = create_dir(dir, &v.hash[0..2]).await?;
                let url = format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    &v.hash[0..2],
                    v.hash
                );

                set.spawn(download_file(path.join(&v.hash), url));
            }

            let mut ok_assets = 0;
            let mut err_assets = 0;

            while let Some(res) = set.join_next().await {
                // FIXME
                let result = res.unwrap();
                if let Err(_err) = result {
                    err_assets += 1;
                } else {
                    ok_assets += 1;
                }
            }

            info!(
                "Chunk downloaded: OK - {}; MISSING - {}",
                ok_assets, err_assets
            );
        }

        Ok(())
    }
}
