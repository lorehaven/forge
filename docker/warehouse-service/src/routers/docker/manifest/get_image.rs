use crate::domain::docker_error;
use crate::routers::docker::{
    manifest_path, repository_path, validate_digest, validate_tag_reference,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use quench_starter::prelude::error;

#[get("/{name:.+}/manifests/{reference}")]
pub async fn handle(req: HttpRequest, path: web::Path<(String, String)>) -> impl Responder {
    let (name, reference) = path.into_inner();

    let resolved = match resolve_manifest_response(&req, &name, &reference).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    HttpResponse::Ok()
        .append_header(("Content-Type", resolved.media_type))
        .append_header(("Docker-Content-Digest", resolved.digest))
        .append_header(("Content-Length", resolved.data.len()))
        .body(resolved.data)
}

pub(super) struct ResolvedManifestResponse {
    pub(super) data: Vec<u8>,
    pub(super) media_type: &'static str,
    pub(super) digest: String,
}

pub(super) async fn resolve_manifest_response(
    req: &HttpRequest,
    name: &str,
    reference: &str,
) -> Result<ResolvedManifestResponse, HttpResponse> {
    let repo_path = repository_path(name).ok_or_else(|| {
        error::response(
            actix_web::http::StatusCode::BAD_REQUEST,
            docker_error::NAME_UNKNOWN,
            "invalid repository name",
        )
    })?;

    // Resolve reference → digest
    let digest = if reference.starts_with("sha256:") {
        reference.to_string()
    } else {
        if !validate_tag_reference(reference) {
            return Err(error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                error::UNSUPPORTED,
                "invalid manifest reference",
            ));
        }
        let tag_path = repo_path.join("tags").join(reference);
        match tokio::fs::read_to_string(&tag_path).await {
            Ok(d) => d.trim().to_string(),
            Err(_) => {
                return Err(error::response(
                    actix_web::http::StatusCode::NOT_FOUND,
                    docker_error::MANIFEST_UNKNOWN,
                    "manifest unknown",
                ));
            }
        }
    };

    if !validate_digest(&digest) {
        return Err(error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        ));
    }

    let Some(manifest_path) = manifest_path(&digest) else {
        return Err(error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        ));
    };
    let data = match tokio::fs::read(&manifest_path).await {
        Ok(d) => d,
        Err(_) => {
            return Err(error::response(
                actix_web::http::StatusCode::NOT_FOUND,
                docker_error::MANIFEST_UNKNOWN,
                "manifest unknown",
            ));
        }
    };

    // Detect stored media type from JSON
    let stored_media_type = match detect_manifest_media_type(&data) {
        Some(mt) => mt,
        None => {
            return Err(error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                error::UNSUPPORTED,
                "manifest media type unsupported",
            ));
        }
    };

    // Strict RFC negotiation
    let accept = req
        .headers()
        .get("Accept")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    // Docker clients probe with varying Accept headers; prefer serving what's stored over a strict 406.
    let chosen = negotiate_media_type(accept, &[stored_media_type]).unwrap_or(stored_media_type);

    Ok(ResolvedManifestResponse {
        data,
        media_type: chosen,
        digest,
    })
}

const DOCKER_MANIFEST_V2: &str = "application/vnd.docker.distribution.manifest.v2+json";
const DOCKER_MANIFEST_LIST_V2: &str = "application/vnd.docker.distribution.manifest.list.v2+json";
const OCI_IMAGE_MANIFEST_V1: &str = "application/vnd.oci.image.manifest.v1+json";
const OCI_IMAGE_INDEX_V1: &str = "application/vnd.oci.image.index.v1+json";

fn detect_manifest_media_type(data: &[u8]) -> Option<&'static str> {
    let v: serde_json::Value = serde_json::from_slice(data).ok()?;

    if let Some(media_type) = v.get("mediaType").and_then(|m| m.as_str()) {
        return match media_type {
            DOCKER_MANIFEST_V2 => Some(DOCKER_MANIFEST_V2),
            DOCKER_MANIFEST_LIST_V2 => Some(DOCKER_MANIFEST_LIST_V2),
            OCI_IMAGE_MANIFEST_V1 => Some(OCI_IMAGE_MANIFEST_V1),
            OCI_IMAGE_INDEX_V1 => Some(OCI_IMAGE_INDEX_V1),
            _ => None,
        };
    }

    if v.get("schemaVersion").and_then(|s| s.as_u64()) != Some(2) {
        return None;
    }

    if let Some(manifests) = v.get("manifests").and_then(|m| m.as_array()) {
        let is_oci = manifests.iter().any(|m| {
            m.get("mediaType")
                .and_then(|x| x.as_str())
                .map(|s| s.starts_with("application/vnd.oci."))
                .unwrap_or(false)
        });
        return Some(if is_oci {
            OCI_IMAGE_INDEX_V1
        } else {
            DOCKER_MANIFEST_LIST_V2
        });
    }

    if v.get("config").is_some() && v.get("layers").and_then(|l| l.as_array()).is_some() {
        let config_is_oci = v
            .get("config")
            .and_then(|c| c.get("mediaType"))
            .and_then(|x| x.as_str())
            .map(|s| s.starts_with("application/vnd.oci."))
            .unwrap_or(false);
        let layers_is_oci = v
            .get("layers")
            .and_then(|l| l.as_array())
            .map(|layers| {
                layers.iter().any(|layer| {
                    layer
                        .get("mediaType")
                        .and_then(|x| x.as_str())
                        .map(|s| s.starts_with("application/vnd.oci."))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        return Some(if config_is_oci || layers_is_oci {
            OCI_IMAGE_MANIFEST_V1
        } else {
            DOCKER_MANIFEST_V2
        });
    }

    None
}

fn negotiate_media_type(accept: &str, available: &[&'static str]) -> Option<&'static str> {
    if accept.is_empty() {
        return available.first().copied();
    }

    let mut ranges = parse_accept(accept);

    // Highest q first
    ranges.sort_by(|a, b| b.q.partial_cmp(&a.q).unwrap());

    for range in ranges {
        for &candidate in available {
            if media_match(&range.value, candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

fn media_match(range: &str, candidate: &str) -> bool {
    if range == "*/*" {
        return true;
    }

    if range.eq_ignore_ascii_case(candidate) {
        return true;
    }

    if equivalent_manifest_media_types(range, candidate) {
        return true;
    }

    if let Some(prefix) = range.strip_suffix("/*") {
        return candidate
            .get(..prefix.len())
            .map(|head| head.eq_ignore_ascii_case(prefix))
            .unwrap_or(false);
    }

    false
}

fn equivalent_manifest_media_types(requested: &str, candidate: &str) -> bool {
    matches!(
        (requested, candidate),
        (DOCKER_MANIFEST_V2, OCI_IMAGE_MANIFEST_V1)
            | (OCI_IMAGE_MANIFEST_V1, DOCKER_MANIFEST_V2)
            | (DOCKER_MANIFEST_LIST_V2, OCI_IMAGE_INDEX_V1)
            | (OCI_IMAGE_INDEX_V1, DOCKER_MANIFEST_LIST_V2)
    )
}

#[derive(Debug)]
struct MediaRange {
    value: String,
    q: f32,
}

fn parse_accept(header: &str) -> Vec<MediaRange> {
    header
        .split(',')
        .filter_map(|part| {
            let mut sections = part.trim().split(';');

            let value = sections.next()?.trim().to_ascii_lowercase();
            if value.is_empty() {
                return None;
            }
            let mut q = 1.0;

            for s in sections {
                let s = s.trim();
                if let Some(v) = s.strip_prefix("q=") {
                    q = v.parse().unwrap_or(1.0);
                }
            }

            Some(MediaRange { value, q })
        })
        .collect()
}
