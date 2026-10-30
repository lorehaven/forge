//! A storage in memory, laid out as `anvil docs release` lays it out.

use lectern_service::source::{Source, SourceError};
use quench_core::async_trait;
use quench_serde::json;
use std::collections::BTreeMap;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub struct MemorySource {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    pub down: Mutex<bool>,
}

impl MemorySource {
    pub fn put(&self, path: &str, data: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), data.to_vec());
    }

    pub fn remove_book(&self, name: &str) {
        let prefix = format!("{name}/");
        self.files
            .lock()
            .unwrap()
            .retain(|path, _| !path.starts_with(&prefix));
    }

    /// Releases `version` of `name` made of `files`; `baseurl` is what the manifest says it was built with.
    pub fn release(
        &self,
        name: &str,
        version: &str,
        files: &[(&str, &str)],
        baseurl: Option<&str>,
    ) {
        let (archive_name, archive, manifest) = pack(name, version, files, baseurl);
        let dir = format!("{name}/{version}");
        self.put(&format!("{dir}/{archive_name}"), &archive);
        self.put(&format!("{dir}/manifest.json"), manifest.as_bytes());
        // `latest.json` is the manifest plus where the files are, as anvil writes it.
        let mut latest = quench_serde::json::from_str::<quench_serde::Value>(&manifest).unwrap();
        latest.as_object_mut().unwrap().insert(
            "archive_path".to_string(),
            quench_serde::Value::from(format!("{dir}/{archive_name}").as_str()),
        );
        self.put(
            &format!("{name}/latest.json"),
            latest.to_string().as_bytes(),
        );
    }
}

pub fn pack(
    name: &str,
    version: &str,
    files: &[(&str, &str)],
    baseurl: Option<&str>,
) -> (String, Vec<u8>, String) {
    let mut tar = quench_archive::tar::Builder::new();
    for (path, body) in files {
        tar.append_file(path, body.as_bytes()).unwrap();
    }
    let archive = quench_archive::gzip::compress(&tar.finish());
    let archive_name = format!("{name}-{version}.tar.gz");
    let mut manifest = json!({
        "book": name,
        "title": name.to_uppercase(),
        "version": version,
        "built": "2026-10-30T20:20:33Z",
        "archive": archive_name.as_str(),
        "archive_sha256": quench_core::hex::encode(quench_crypto::hash::sha256(&archive)),
    });
    if let Some(baseurl) = baseurl {
        manifest
            .as_object_mut()
            .unwrap()
            .insert("baseurl".to_string(), quench_serde::Value::from(baseurl));
    }
    (archive_name, archive, manifest.to_string())
}

#[async_trait]
impl Source for MemorySource {
    async fn books(&self) -> Result<Vec<String>, SourceError> {
        if *self.down.lock().unwrap() {
            return Err(SourceError("storage unreachable".to_string()));
        }
        let mut names: Vec<String> = self
            .files
            .lock()
            .unwrap()
            .keys()
            .filter_map(|path| path.split_once('/').map(|(book, _)| book.to_string()))
            .collect();
        names.dedup();
        Ok(names)
    }

    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, SourceError> {
        Ok(self.files.lock().unwrap().get(path).cloned())
    }
}
