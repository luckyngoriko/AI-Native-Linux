//! `WebAppsBridge` — Web renderer ↔ aios-apps gRPC client (T-149 §2).
//!
//! Connects the Web renderer to the `aios-apps` `AppsService` gRPC surface
//! and compiles `ListPackages` / `GetPackage` responses into
//! [`WebRenderTree`] shapes using the 19-variant closed [`NodeKind`]
//! vocabulary.

use aios_apps::service::proto::apps_service_client::AppsServiceClient;
use aios_apps::service::proto::GetPackageRequest;
use tonic::Request;

use crate::error::WebRendererError;
use crate::integration::renderer_parity::{dom_tag_for, WebRenderTree, WebRenderTreeEntry};
use crate::NodeKind;

/// Bridge from the Web renderer to the `aios-apps` gRPC `AppsService`.
///
/// Holds a tonic [`AppsServiceClient`] and provides rendering methods that
/// compile service responses into [`WebRenderTree`] shapes without pulling in
/// any DOM/JS dependency.
pub struct WebAppsBridge {
    client: AppsServiceClient<tonic::transport::Channel>,
}

impl std::fmt::Debug for WebAppsBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebAppsBridge").finish_non_exhaustive()
    }
}

impl WebAppsBridge {
    /// Connect to an `AppsService` at the given endpoint.
    ///
    /// # Errors
    ///
    /// - `Internal` when `endpoint` is empty.
    /// - `Internal` when the endpoint URL is invalid or the gRPC transport
    ///   cannot establish a channel.
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, WebRendererError> {
        let endpoint_str: String = endpoint.into();
        if endpoint_str.is_empty() {
            return Err(WebRendererError::Internal(
                "empty endpoint: cannot connect AppsService".into(),
            ));
        }
        let channel = tonic::transport::Endpoint::new(endpoint_str)
            .map_err(|e| WebRendererError::Internal(format!("invalid endpoint URL: {e}")))?
            .connect()
            .await
            .map_err(|e| WebRendererError::Internal(format!("gRPC connect failed: {e}")))?;
        let client = AppsServiceClient::new(channel);
        Ok(Self { client })
    }

    /// Call `ListPackages` and compile the response into a [`WebRenderTree`].
    ///
    /// The root is a [`NodeKind::List`] labelled `"Packages"`. Each package
    /// becomes a [`NodeKind::Card`] child carrying its name + version, with
    /// a [`NodeKind::Text`] grandchild for the package id.
    ///
    /// When the list is empty the tree still has a `List` root with zero
    /// children.
    ///
    /// # Errors
    ///
    /// Returns `Internal` when the RPC fails at the transport or application
    /// layer.
    pub async fn render_package_list_as_web_tree(
        &mut self,
    ) -> Result<WebRenderTree, WebRendererError> {
        let request = Request::new(());
        let resp = self
            .client
            .list_packages(request)
            .await
            .map_err(|e| WebRendererError::Internal(format!("ListPackages RPC failed: {e}")))?;
        let list = resp.into_inner();
        let children: Vec<WebRenderTreeEntry> = list
            .packages
            .iter()
            .map(|pkg| WebRenderTreeEntry {
                kind: NodeKind::Card,
                dom_tag: dom_tag_for(NodeKind::Card),
                label: format!("{} v{}", pkg.name, pkg.version),
                children: vec![WebRenderTreeEntry {
                    kind: NodeKind::Text,
                    dom_tag: dom_tag_for(NodeKind::Text),
                    label: format!("id: {}", pkg.package_id),
                    children: vec![],
                }],
            })
            .collect();

        let root = WebRenderTreeEntry {
            kind: NodeKind::List,
            dom_tag: dom_tag_for(NodeKind::List),
            label: "Packages".into(),
            children,
        };
        Ok(WebRenderTree { root })
    }

    /// Call `GetPackage` and compile the response into a [`WebRenderTree`].
    ///
    /// The root is a [`NodeKind::Card`] carrying the package name. Two child
    /// [`NodeKind::Text`] nodes carry the version and package id.
    ///
    /// # Errors
    ///
    /// Returns `Internal` when the RPC fails, including when the package is
    /// not found (the server returns `NOT_FOUND`, which is mapped to
    /// `Internal`).
    pub async fn render_package_show_as_web_tree(
        &mut self,
        pkg_id: &str,
    ) -> Result<WebRenderTree, WebRendererError> {
        let request = Request::new(GetPackageRequest {
            package_id: pkg_id.to_string(),
        });
        let resp = self.client.get_package(request).await.map_err(|status| {
            WebRendererError::Internal(format!("GetPackage RPC failed for '{pkg_id}': {status}"))
        })?;
        let pkg = resp.into_inner().package.ok_or_else(|| {
            WebRendererError::Internal(format!("GetPackage returned empty payload for '{pkg_id}'"))
        })?;

        let root = WebRenderTreeEntry {
            kind: NodeKind::Card,
            dom_tag: dom_tag_for(NodeKind::Card),
            label: pkg.name.clone(),
            children: vec![
                WebRenderTreeEntry {
                    kind: NodeKind::Text,
                    dom_tag: dom_tag_for(NodeKind::Text),
                    label: format!("version: {}", pkg.version),
                    children: vec![],
                },
                WebRenderTreeEntry {
                    kind: NodeKind::Text,
                    dom_tag: dom_tag_for(NodeKind::Text),
                    label: format!("id: {}", pkg.package_id),
                    children: vec![],
                },
            ],
        };
        Ok(WebRenderTree { root })
    }
}
