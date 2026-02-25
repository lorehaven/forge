use crate::domain::docker_error;
use crate::routers::docker::{
    manifest_path, repository_path, validate_digest, validate_tag_reference,
};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[utoipa::path(
    get,
    operation_id = "get_image",
    tags = ["docker - manifest"],
    path = "/{name}/manifests/{reference}",
    params(
        ("Accept" = Option<String>, Header, description = "Manifest media types supported by client"),
        ("name" = String, Path, description = "Repository name (may contain slashes)"),
        ("reference" = String, Path, description = "Tag or digest of the target manifest"),
    ),
    responses(
        (
            status = 200,
            description = "Manifest fetched successfully",
            content(
                ("application/vnd.docker.distribution.manifest.v2+json"),
                ("application/vnd.oci.image.manifest.v1+json"),
                ("application/vnd.docker.distribution.manifest.list.v2+json"),
                ("application/vnd.oci.image.index.v1+json")
            ),
            headers(
                ("Docker-Content-Digest" = String),
                ("Content-Length" = u64),
            )
        ),
        (status = 400, description = "Invalid name or reference"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Access denied"),
        (status = 404, description = "Repository or manifest not found"),
        (status = 429, description = "Too many requests"),
    )
)]
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
        docker_error::response(
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
            return Err(docker_error::response(
                actix_web::http::StatusCode::BAD_REQUEST,
                docker_error::UNSUPPORTED,
                "invalid manifest reference",
            ));
        }
        let tag_path = repo_path.join("tags").join(reference);
        match tokio::fs::read_to_string(&tag_path).await {
            Ok(d) => d.trim().to_string(),
            Err(_) => {
                return Err(docker_error::response(
                    actix_web::http::StatusCode::NOT_FOUND,
                    docker_error::MANIFEST_UNKNOWN,
                    "manifest unknown",
                ));
            }
        }
    };

    if !validate_digest(&digest) {
        return Err(docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        ));
    }

    let Some(manifest_path) = manifest_path(&digest) else {
        return Err(docker_error::response(
            actix_web::http::StatusCode::NOT_FOUND,
            docker_error::MANIFEST_UNKNOWN,
            "manifest unknown",
        ));
    };
    let data = match tokio::fs::read(&manifest_path).await {
        Ok(d) => d,
        Err(_) => {
            return Err(docker_error::response(
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
            return Err(docker_error::response(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                docker_error::UNSUPPORTED,
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

    let mut response_data = data;
    let mut response_media_type = stored_media_type;
    let mut skip_negotiation = false;

    // If a manifest list/index is requested, resolve and return the matching image manifest.
    if is_index_media_type(stored_media_type) && accept_requests_index(accept) {
        let platform = client_platform(req);
        let resolved = match resolve_platform_manifest(&response_data, &platform).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Err(docker_error::response(
                    actix_web::http::StatusCode::NOT_FOUND,
                    docker_error::MANIFEST_UNKNOWN,
                    "no manifest found for requested platform",
                ));
            }
            Err(()) => {
                return Err(docker_error::response(
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                    docker_error::UNSUPPORTED,
                    "manifest index resolution failed",
                ));
            }
        };

        response_data = resolved.data;
        response_media_type = resolved.media_type;
        skip_negotiation = true;
    }

    let chosen = if skip_negotiation {
        response_media_type
    } else {
        match negotiate_media_type(accept, &[response_media_type]) {
            Some(mt) => mt,
            None => {
                return Err(docker_error::response(
                    actix_web::http::StatusCode::NOT_ACCEPTABLE,
                    docker_error::UNSUPPORTED,
                    "requested media type is not supported",
                ));
            }
        }
    };

    // Recompute digest
    let mut hasher = Sha256::new();
    hasher.update(&response_data);
    let computed = format!("sha256:{:x}", hasher.finalize());

    Ok(ResolvedManifestResponse {
        data: response_data,
        media_type: chosen,
        digest: computed,
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

#[derive(Debug, Clone)]
struct ClientPlatform {
    os: String,
    architecture: String,
    variant: Option<String>,
}

#[derive(Deserialize)]
struct ManifestIndex {
    manifests: Vec<ManifestDescriptor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestDescriptor {
    digest: String,
    media_type: String,
    platform: Option<DescriptorPlatform>,
}

#[derive(Deserialize)]
struct DescriptorPlatform {
    os: String,
    architecture: String,
    variant: Option<String>,
}

struct ResolvedIndexManifest {
    data: Vec<u8>,
    media_type: &'static str,
}

fn is_index_media_type(media_type: &str) -> bool {
    media_type == DOCKER_MANIFEST_LIST_V2 || media_type == OCI_IMAGE_INDEX_V1
}

fn accept_requests_index(accept: &str) -> bool {
    if accept.is_empty() {
        return false;
    }

    parse_accept(accept).into_iter().any(|range| {
        media_match(&range.value, DOCKER_MANIFEST_LIST_V2)
            || media_match(&range.value, OCI_IMAGE_INDEX_V1)
    })
}

fn client_platform(req: &HttpRequest) -> ClientPlatform {
    const PLATFORM_HEADERS: [&str; 2] = ["Docker-Platform", "X-Docker-Platform"];

    for header_name in PLATFORM_HEADERS {
        if let Some(value) = req.headers().get(header_name).and_then(|v| v.to_str().ok())
            && let Some(platform) = parse_platform(value)
        {
            return platform;
        }
    }

    ClientPlatform {
        os: "linux".to_string(),
        architecture: "amd64".to_string(),
        variant: None,
    }
}

fn parse_platform(value: &str) -> Option<ClientPlatform> {
    let mut parts = value.split('/');
    let os = parts.next()?.trim();
    let architecture = parts.next()?.trim();
    let variant = parts.next().map(|v| v.trim().to_string());

    if os.is_empty() || architecture.is_empty() || parts.next().is_some() {
        return None;
    }

    Some(ClientPlatform {
        os: os.to_string(),
        architecture: architecture.to_string(),
        variant,
    })
}

async fn resolve_platform_manifest(
    index_data: &[u8],
    platform: &ClientPlatform,
) -> Result<Option<ResolvedIndexManifest>, ()> {
    let index: ManifestIndex = serde_json::from_slice(index_data).map_err(|_| ())?;

    let descriptor = index
        .manifests
        .iter()
        .find(|desc| descriptor_matches_platform(desc, platform));

    let descriptor = match descriptor {
        Some(d) => d,
        None => return Ok(None),
    };

    if !validate_digest(&descriptor.digest) {
        return Err(());
    }

    let media_type = detect_media_type_value(&descriptor.media_type).ok_or(())?;
    let manifest_path = manifest_path(&descriptor.digest).ok_or(())?;
    let data = tokio::fs::read(manifest_path).await.map_err(|_| ())?;

    Ok(Some(ResolvedIndexManifest { data, media_type }))
}

fn descriptor_matches_platform(desc: &ManifestDescriptor, target: &ClientPlatform) -> bool {
    let platform = match &desc.platform {
        Some(p) => p,
        None => return false,
    };

    if platform.os != target.os || platform.architecture != target.architecture {
        return false;
    }

    match (&target.variant, &platform.variant) {
        (Some(a), Some(b)) => a == b,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn detect_media_type_value(media_type: &str) -> Option<&'static str> {
    match media_type {
        DOCKER_MANIFEST_V2 => Some(DOCKER_MANIFEST_V2),
        OCI_IMAGE_MANIFEST_V1 => Some(OCI_IMAGE_MANIFEST_V1),
        DOCKER_MANIFEST_LIST_V2 => Some(DOCKER_MANIFEST_LIST_V2),
        OCI_IMAGE_INDEX_V1 => Some(OCI_IMAGE_INDEX_V1),
        _ => None,
    }
}
