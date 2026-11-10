use crate::worker;

pub fn acquire(&self, build: &BuildInformation) {
    if let BuildInformation { manifest_url: Some(man), manifest_download_url: Some(man_dl), .. } = build {
        self.acquire_from_manifest(man, man_dl)
    }
    let BuildInformation { download_url, hash, .. } = build;
    todo!()
}

async fn old_acquire_from_manifest(&self, man: &str, man_dl: &str) -> anyhow::Result<> {
    let manifest = utils::request_bytes::<SerdeNever>(Url::parse(&utils::sanitize_url(&man))?, None, Method::GET).await?;
    if build.manifest_hash.is_some_and(|hash| hash != Manifest::digest(&*manifest)) {
        log::error!("build manifest hash did not match expected");
        anyhow::bail!("build manifest hash did not match expected");
    }
    let manifest = self.parse_manifest(manifest)?;
    let missing = manifest.iter().enumerate()
        .filter_map(|(index, entry)| (!self.blob_exists(entry.0)).then_some(index))
        .collect::<Vec<_>>();
    log::debug!("manifest contains {} files, we have {} of them", manifest.len(), missing.len());
    if missing.len() > 0 {
        let man_dl = Url::parse(&utils::sanitize_url(&man_dl))?;
        // why is this bytes and not just a json list. what
        let mut body = Vec::new();
        for index in &missing {
            body.extend((*index as u32).to_le_bytes());
        }
        let res = HTTP_CLIENT
            .request(Method::POST, man_dl)
            .body(body)
            .header("Content-Type", "application/octet-stream")
            .header("X-Robust-Download-Protocol", "1")
            .send().await.and_then(|r| r.error_for_status())?;
        let bytes = res.bytes().await?;
        let mut bytes = Cursor::new(&*bytes);
        let mut int_buf = [0u8; 4];
        bytes.read_exact(&mut int_buf)?;
        let flags = u32::from_le_bytes(int_buf);
        for index in &missing {
            bytes.read_exact(&mut int_buf)?;
            let size = u32::from_le_bytes(int_buf) as usize;
            let mut compressed = 0;
            if flags & 1 != 0 {
                bytes.read_exact(&mut int_buf)?;
                compressed = u32::from_le_bytes(int_buf) as usize;
            }
            let mut raw = vec![0; if compressed == 0 { size } else { compressed }];
            bytes.read_exact(&mut raw)?;
            if flags & 1 != 0 && compressed != 0 {
                let mut decoder = StreamingDecoder::new(Cursor::new(raw))?;
                raw = vec![0; size];
                decoder.read_exact(&mut raw)?;
            }
            self.put_blob(manifest[*index].0, raw)?;
        }
    }
    return Ok(manifest)
}

fn acquire_from_manifest(&self, man: &str, man_dl: &str) {
    worker::
}

fn parse_manifest(&self, manifest: bytes::Bytes) -> Result<Vec<(Manifest, PathBuf)>, anyhow::Error> {
    let data = String::from_utf8(manifest.to_vec())?;
    let mut lines = data.lines();
    let Some("Robust Content Manifest 1") = lines.next() else {
        anyhow::bail!("incorrect header string")
    };
    lines.map(|line| {
        let Some((digest, path)) = line.split_once(' ') else { anyhow::bail!("invalid digest/path pair") };
        let hash = digest.parse()?;
        let path = path.parse()?;
        Ok((hash, path))
    }).collect()
}