//! Tool catalog with hardcoded definitions.
//!
//! This module contains the static definitions for all supported external tools,
//! including their download URLs, versions, and metadata.

use super::types::{
    ExecutablePaths, ExternalToolId, PlatformDownload, PlatformUrls, ToolDefinition,
};

// ============================================================================
// UV Definition (Python package installer)
// ============================================================================

const UV_VERSION: &str = "0.5.14";

const UV_URLS: PlatformUrls = PlatformUrls {
    linux_x64: Some(PlatformDownload::new(
        "https://github.com/astral-sh/uv/releases/download/0.5.14/uv-x86_64-unknown-linux-gnu.tar.gz",
        Some("22034760075b92487b326da5aa1a2a3e1917e2e766c12c0fd466fccda77013c7"),
    )),
    linux_arm64: Some(PlatformDownload::new(
        "https://github.com/astral-sh/uv/releases/download/0.5.14/uv-aarch64-unknown-linux-gnu.tar.gz",
        Some("1c9cdb265b0c24ce2e74b7795a00842dc6d487c11ba49aa6c9ca1c784b82755a"),
    )),
    macos_x64: Some(PlatformDownload::new(
        "https://github.com/astral-sh/uv/releases/download/0.5.14/uv-x86_64-apple-darwin.tar.gz",
        Some("8caf91b936ede1167abaebae07c2a1cbb22473355fa0ad7ebb2580307e84fb47"),
    )),
    macos_arm64: Some(PlatformDownload::new(
        "https://github.com/astral-sh/uv/releases/download/0.5.14/uv-aarch64-apple-darwin.tar.gz",
        Some("d548dffc256014c4c8c693e148140a3a21bcc2bf066a35e1d5f0d24c91d32112"),
    )),
    windows_x64: Some(PlatformDownload::new(
        "https://github.com/astral-sh/uv/releases/download/0.5.14/uv-x86_64-pc-windows-msvc.zip",
        Some("ee2468e40320a0a2a36435e66bbd0d861228c4c06767f22d97876528138f4ba0"),
    )),
};

const UV_EXECUTABLE_PATHS: ExecutablePaths = ExecutablePaths {
    linux: "uv",
    macos: "uv",
    windows: "uv.exe",
};

const UV_DEFINITION: ToolDefinition = ToolDefinition {
    id: ExternalToolId::Uv,
    display_name: "UV",
    description: "Fast Python package installer and resolver (required for Python skills)",
    version: UV_VERSION,
    size_mb: 15,
    required_by: &["python", "skills"],
    urls: UV_URLS,
    executable_paths: UV_EXECUTABLE_PATHS,
};

// ============================================================================
// Pandoc Definition
// ============================================================================

const PANDOC_VERSION: &str = "3.6.2";

const PANDOC_URLS: PlatformUrls = PlatformUrls {
    linux_x64: Some(PlatformDownload::new(
        "https://github.com/jgm/pandoc/releases/download/3.6.2/pandoc-3.6.2-linux-amd64.tar.gz",
        None,
    )),
    linux_arm64: Some(PlatformDownload::new(
        "https://github.com/jgm/pandoc/releases/download/3.6.2/pandoc-3.6.2-linux-arm64.tar.gz",
        None,
    )),
    macos_x64: Some(PlatformDownload::new(
        "https://github.com/jgm/pandoc/releases/download/3.6.2/pandoc-3.6.2-x86_64-macOS.zip",
        None,
    )),
    macos_arm64: Some(PlatformDownload::new(
        "https://github.com/jgm/pandoc/releases/download/3.6.2/pandoc-3.6.2-arm64-macOS.zip",
        None,
    )),
    windows_x64: Some(PlatformDownload::new(
        "https://github.com/jgm/pandoc/releases/download/3.6.2/pandoc-3.6.2-windows-x86_64.zip",
        None,
    )),
};

const PANDOC_EXECUTABLE_PATHS: ExecutablePaths = ExecutablePaths {
    linux: "bin/pandoc",
    macos: "bin/pandoc",
    windows: "pandoc.exe",
};

const PANDOC_DEFINITION: ToolDefinition = ToolDefinition {
    id: ExternalToolId::Pandoc,
    display_name: "Pandoc",
    description: "Universal document converter for text extraction from docx and other formats",
    version: PANDOC_VERSION,
    size_mb: 40,
    required_by: &["docx"],
    urls: PANDOC_URLS,
    executable_paths: PANDOC_EXECUTABLE_PATHS,
};

// ============================================================================
// Node.js Definition
// ============================================================================

const NODE_VERSION: &str = "22.12.0";

const NODE_URLS: PlatformUrls = PlatformUrls {
    linux_x64: Some(PlatformDownload::new(
        "https://nodejs.org/dist/v22.12.0/node-v22.12.0-linux-x64.tar.xz",
        None,
    )),
    linux_arm64: Some(PlatformDownload::new(
        "https://nodejs.org/dist/v22.12.0/node-v22.12.0-linux-arm64.tar.xz",
        None,
    )),
    macos_x64: Some(PlatformDownload::new(
        "https://nodejs.org/dist/v22.12.0/node-v22.12.0-darwin-x64.tar.gz",
        None,
    )),
    macos_arm64: Some(PlatformDownload::new(
        "https://nodejs.org/dist/v22.12.0/node-v22.12.0-darwin-arm64.tar.gz",
        None,
    )),
    windows_x64: Some(PlatformDownload::new(
        "https://nodejs.org/dist/v22.12.0/node-v22.12.0-win-x64.zip",
        None,
    )),
};

const NODE_EXECUTABLE_PATHS: ExecutablePaths = ExecutablePaths {
    linux: "bin/node",
    macos: "bin/node",
    windows: "node.exe",
};

const NODE_DEFINITION: ToolDefinition = ToolDefinition {
    id: ExternalToolId::Node,
    display_name: "Node.js",
    description: "JavaScript runtime for document and presentation creation (docx, pptx)",
    version: NODE_VERSION,
    size_mb: 25,
    required_by: &["docx", "pptx"],
    urls: NODE_URLS,
    executable_paths: NODE_EXECUTABLE_PATHS,
};

// ============================================================================
// LibreOffice Definition
// ============================================================================

const LIBREOFFICE_VERSION: &str = "fresh";

const LIBREOFFICE_URLS: PlatformUrls = PlatformUrls {
    linux_x64: Some(PlatformDownload::new(
        "https://appimages.libreitalia.org/LibreOffice-fresh.basic-x86_64.AppImage",
        None,
    )),
    linux_arm64: None,
    macos_x64: None,
    macos_arm64: None,
    windows_x64: None,
};

const LIBREOFFICE_EXECUTABLE_PATHS: ExecutablePaths = ExecutablePaths {
    linux: "soffice.AppImage",
    macos: "Contents/MacOS/soffice",
    windows: "program/soffice.exe",
};

const LIBREOFFICE_DEFINITION: ToolDefinition = ToolDefinition {
    id: ExternalToolId::LibreOffice,
    display_name: "LibreOffice",
    description: "Office suite for formula recalculation, visual validation, and PDF conversion",
    version: LIBREOFFICE_VERSION,
    size_mb: 280,
    required_by: &["xlsx", "docx", "pptx"],
    urls: LIBREOFFICE_URLS,
    executable_paths: LIBREOFFICE_EXECUTABLE_PATHS,
};

// ============================================================================
// Catalog Access
// ============================================================================

/// Returns the definition for a specific tool.
pub fn get_tool_definition(id: ExternalToolId) -> &'static ToolDefinition {
    match id {
        ExternalToolId::Uv => &UV_DEFINITION,
        ExternalToolId::Pandoc => &PANDOC_DEFINITION,
        ExternalToolId::Node => &NODE_DEFINITION,
        ExternalToolId::LibreOffice => &LIBREOFFICE_DEFINITION,
    }
}

/// Returns definitions for all available tools.
pub fn get_all_tool_definitions() -> Vec<&'static ToolDefinition> {
    vec![
        &UV_DEFINITION,
        &PANDOC_DEFINITION,
        &NODE_DEFINITION,
        &LIBREOFFICE_DEFINITION,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external_tools::types::Platform;

    #[test]
    fn test_get_tool_definition() {
        let uv = get_tool_definition(ExternalToolId::Uv);
        assert_eq!(uv.id, ExternalToolId::Uv);
        assert_eq!(uv.display_name, "UV");
        assert_eq!(uv.version, "0.5.14");

        let pandoc = get_tool_definition(ExternalToolId::Pandoc);
        assert_eq!(pandoc.id, ExternalToolId::Pandoc);
        assert_eq!(pandoc.display_name, "Pandoc");
        assert_eq!(pandoc.version, "3.6.2");

        let node = get_tool_definition(ExternalToolId::Node);
        assert_eq!(node.id, ExternalToolId::Node);
        assert_eq!(node.display_name, "Node.js");
        assert_eq!(node.version, "22.12.0");

        let libreoffice = get_tool_definition(ExternalToolId::LibreOffice);
        assert_eq!(libreoffice.id, ExternalToolId::LibreOffice);
        assert_eq!(libreoffice.display_name, "LibreOffice");
        assert_eq!(libreoffice.version, "fresh");
    }

    #[test]
    fn test_get_all_tool_definitions() {
        let defs = get_all_tool_definitions();
        assert_eq!(defs.len(), 4);
    }

    #[test]
    fn test_uv_has_all_platform_urls() {
        let uv = get_tool_definition(ExternalToolId::Uv);

        assert!(uv.urls.get(Platform::LinuxX64).is_some());
        assert!(uv.urls.get(Platform::LinuxArm64).is_some());
        assert!(uv.urls.get(Platform::MacosX64).is_some());
        assert!(uv.urls.get(Platform::MacosArm64).is_some());
        assert!(uv.urls.get(Platform::WindowsX64).is_some());
    }

    #[test]
    fn test_uv_has_sha256_checksums() {
        let uv = get_tool_definition(ExternalToolId::Uv);

        assert!(uv.urls.get(Platform::LinuxX64).unwrap().sha256.is_some());
        assert!(uv.urls.get(Platform::LinuxArm64).unwrap().sha256.is_some());
        assert!(uv.urls.get(Platform::MacosX64).unwrap().sha256.is_some());
        assert!(uv.urls.get(Platform::MacosArm64).unwrap().sha256.is_some());
        assert!(uv.urls.get(Platform::WindowsX64).unwrap().sha256.is_some());
    }

    #[test]
    fn test_uv_executable_paths() {
        let uv = get_tool_definition(ExternalToolId::Uv);

        assert_eq!(uv.get_executable_path(Platform::LinuxX64), "uv");
        assert_eq!(uv.get_executable_path(Platform::LinuxArm64), "uv");
        assert_eq!(uv.get_executable_path(Platform::MacosX64), "uv");
        assert_eq!(uv.get_executable_path(Platform::MacosArm64), "uv");
        assert_eq!(uv.get_executable_path(Platform::WindowsX64), "uv.exe");
    }

    #[test]
    fn test_uv_archive_format() {
        let uv = get_tool_definition(ExternalToolId::Uv);

        // Unix platforms use tar.gz
        assert_eq!(
            uv.get_archive_format(Platform::LinuxX64),
            Some(crate::external_tools::types::ArchiveFormat::TarGz)
        );
        assert_eq!(
            uv.get_archive_format(Platform::MacosArm64),
            Some(crate::external_tools::types::ArchiveFormat::TarGz)
        );
        // Windows uses zip
        assert_eq!(
            uv.get_archive_format(Platform::WindowsX64),
            Some(crate::external_tools::types::ArchiveFormat::Zip)
        );
    }

    #[test]
    fn test_pandoc_has_all_platform_urls() {
        let pandoc = get_tool_definition(ExternalToolId::Pandoc);

        assert!(pandoc.urls.get(Platform::LinuxX64).is_some());
        assert!(pandoc.urls.get(Platform::LinuxArm64).is_some());
        assert!(pandoc.urls.get(Platform::MacosX64).is_some());
        assert!(pandoc.urls.get(Platform::MacosArm64).is_some());
        assert!(pandoc.urls.get(Platform::WindowsX64).is_some());
    }

    #[test]
    fn test_pandoc_has_no_sha256_checksums() {
        let pandoc = get_tool_definition(ExternalToolId::Pandoc);

        assert!(pandoc
            .urls
            .get(Platform::LinuxX64)
            .unwrap()
            .sha256
            .is_none());
        assert!(pandoc
            .urls
            .get(Platform::LinuxArm64)
            .unwrap()
            .sha256
            .is_none());
        assert!(pandoc
            .urls
            .get(Platform::MacosX64)
            .unwrap()
            .sha256
            .is_none());
        assert!(pandoc
            .urls
            .get(Platform::MacosArm64)
            .unwrap()
            .sha256
            .is_none());
        assert!(pandoc
            .urls
            .get(Platform::WindowsX64)
            .unwrap()
            .sha256
            .is_none());
    }

    #[test]
    fn test_pandoc_executable_paths() {
        let pandoc = get_tool_definition(ExternalToolId::Pandoc);

        assert_eq!(pandoc.get_executable_path(Platform::LinuxX64), "bin/pandoc");
        assert_eq!(
            pandoc.get_executable_path(Platform::LinuxArm64),
            "bin/pandoc"
        );
        assert_eq!(pandoc.get_executable_path(Platform::MacosX64), "bin/pandoc");
        assert_eq!(
            pandoc.get_executable_path(Platform::MacosArm64),
            "bin/pandoc"
        );
        assert_eq!(
            pandoc.get_executable_path(Platform::WindowsX64),
            "pandoc.exe"
        );
    }

    #[test]
    fn test_pandoc_archive_format() {
        let pandoc = get_tool_definition(ExternalToolId::Pandoc);

        assert_eq!(
            pandoc.get_archive_format(Platform::LinuxX64),
            Some(crate::external_tools::types::ArchiveFormat::TarGz)
        );
        assert_eq!(
            pandoc.get_archive_format(Platform::MacosArm64),
            Some(crate::external_tools::types::ArchiveFormat::Zip)
        );
        assert_eq!(
            pandoc.get_archive_format(Platform::WindowsX64),
            Some(crate::external_tools::types::ArchiveFormat::Zip)
        );
    }

    #[test]
    fn test_node_has_all_platform_urls() {
        let node = get_tool_definition(ExternalToolId::Node);

        assert!(node.urls.get(Platform::LinuxX64).is_some());
        assert!(node.urls.get(Platform::LinuxArm64).is_some());
        assert!(node.urls.get(Platform::MacosX64).is_some());
        assert!(node.urls.get(Platform::MacosArm64).is_some());
        assert!(node.urls.get(Platform::WindowsX64).is_some());
    }

    #[test]
    fn test_node_has_no_sha256_checksums() {
        let node = get_tool_definition(ExternalToolId::Node);

        assert!(node.urls.get(Platform::LinuxX64).unwrap().sha256.is_none());
        assert!(node
            .urls
            .get(Platform::LinuxArm64)
            .unwrap()
            .sha256
            .is_none());
        assert!(node.urls.get(Platform::MacosX64).unwrap().sha256.is_none());
        assert!(node
            .urls
            .get(Platform::MacosArm64)
            .unwrap()
            .sha256
            .is_none());
        assert!(node
            .urls
            .get(Platform::WindowsX64)
            .unwrap()
            .sha256
            .is_none());
    }

    #[test]
    fn test_node_executable_paths() {
        let node = get_tool_definition(ExternalToolId::Node);

        assert_eq!(node.get_executable_path(Platform::LinuxX64), "bin/node");
        assert_eq!(node.get_executable_path(Platform::LinuxArm64), "bin/node");
        assert_eq!(node.get_executable_path(Platform::MacosX64), "bin/node");
        assert_eq!(node.get_executable_path(Platform::MacosArm64), "bin/node");
        assert_eq!(node.get_executable_path(Platform::WindowsX64), "node.exe");
    }

    #[test]
    fn test_node_archive_format() {
        let node = get_tool_definition(ExternalToolId::Node);

        assert_eq!(
            node.get_archive_format(Platform::LinuxX64),
            Some(crate::external_tools::types::ArchiveFormat::TarXz)
        );
        assert_eq!(
            node.get_archive_format(Platform::MacosArm64),
            Some(crate::external_tools::types::ArchiveFormat::TarGz)
        );
        assert_eq!(
            node.get_archive_format(Platform::WindowsX64),
            Some(crate::external_tools::types::ArchiveFormat::Zip)
        );
    }

    #[test]
    fn test_libreoffice_urls() {
        let libreoffice = get_tool_definition(ExternalToolId::LibreOffice);

        assert!(libreoffice.urls.get(Platform::LinuxX64).is_some());
        assert!(libreoffice.urls.get(Platform::LinuxArm64).is_none());
        assert!(libreoffice.urls.get(Platform::MacosX64).is_none());
        assert!(libreoffice.urls.get(Platform::MacosArm64).is_none());
        assert!(libreoffice.urls.get(Platform::WindowsX64).is_none());
    }

    #[test]
    fn test_libreoffice_executable_paths() {
        let libreoffice = get_tool_definition(ExternalToolId::LibreOffice);

        assert_eq!(
            libreoffice.get_executable_path(Platform::LinuxX64),
            "soffice.AppImage"
        );
        assert_eq!(
            libreoffice.get_executable_path(Platform::LinuxArm64),
            "soffice.AppImage"
        );
        assert_eq!(
            libreoffice.get_executable_path(Platform::MacosX64),
            "Contents/MacOS/soffice"
        );
        assert_eq!(
            libreoffice.get_executable_path(Platform::MacosArm64),
            "Contents/MacOS/soffice"
        );
        assert_eq!(
            libreoffice.get_executable_path(Platform::WindowsX64),
            "program/soffice.exe"
        );
    }

    #[test]
    fn test_libreoffice_archive_format() {
        let libreoffice = get_tool_definition(ExternalToolId::LibreOffice);

        assert_eq!(
            libreoffice.get_archive_format(Platform::LinuxX64),
            Some(crate::external_tools::types::ArchiveFormat::AppImage)
        );
        assert_eq!(libreoffice.get_archive_format(Platform::WindowsX64), None);
    }
}
