use quick_xml::de::from_str;
use serde::Deserialize;
use semver::Version;
use std::fs;
use std::io;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = Path::new("manifest.xml");
    if !manifest_path.exists() {
        eprintln!("Error: manifest.xml not found in working directory");
        std::process::exit(1);
    }

    let mut manifest_str = fs::read_to_string(manifest_path)?;

    let (tool_id, old_version) = match parse_manifest(&manifest_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse manifest.xml: {e}");
            std::process::exit(1);
        }
    };

    let mut version = Version::parse(&old_version)?;
    version.minor += 1;
    // zero out patch if not present
    if version.build.is_empty() && version.pre.is_empty() {
        version.patch = 0;
    }
    let new_version = version.to_string();

    manifest_str = manifest_str.replace(
        &format!("<Version>{}</Version>", old_version),
        &format!("<Version>{}</Version>", new_version),
    );
    fs::write(manifest_path, &manifest_str)?;

    let release_dir = Path::new("release");
    fs::create_dir_all(release_dir)?;

    let folder_name = format!("{}.xrnx", tool_id);
    let plugin_dir = release_dir.join(&folder_name);
    if plugin_dir.exists() {
        fs::remove_dir_all(&plugin_dir)?;
    }
    fs::create_dir_all(&plugin_dir)?;

    copy_sources(&plugin_dir)?;
    fs::copy(manifest_path, plugin_dir.join("manifest.xml"))?;

    let output_zip = release_dir.join(&folder_name);
    zip_dir(&plugin_dir, &output_zip)?;
    println!("Created {}", output_zip.display());
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    #[serde(rename = "@doc_version")]
    doc_version: Option<u32>,
    api_version: Option<u32>,
    author: Option<String>,
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

#[derive(Debug)]
enum ManifestError {
    Xml(quick_xml::DeError),
    MissingField(&'static str),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Xml(e) => write!(f, "XML error: {e}"),
            ManifestError::MissingField(field) => write!(f, "missing required field `{field}`"),
        }
    }
}

impl std::error::Error for ManifestError {}

fn parse_manifest(contents: &str) -> Result<(String, String), ManifestError> {
    let manifest: Manifest = from_str(contents).map_err(ManifestError::Xml)?;

    let id = manifest.id.ok_or(ManifestError::MissingField("Id"))?;
    let version = manifest.version.ok_or(ManifestError::MissingField("Version"))?;

    Ok((id, version))
}

fn copy_sources(dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(".")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "lua").unwrap_or(false) {
            let file_name = path.file_name().unwrap();
            fs::copy(&path, dest.join(file_name))?;
        }
    }
    let readme_lower = Path::new("readme.md");
    let readme_upper = Path::new("README.md");
    if readme_lower.exists() {
        fs::copy(readme_lower, dest.join("README.md"))?;
    } else if readme_upper.exists() {
        fs::copy(readme_upper, dest.join("README.md"))?;
    }
    Ok(())
}

fn zip_dir(dir: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let folder = dir
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "invalid path"))?;
    let status = std::process::Command::new("zip")
        .arg("-r")
        .arg(out)
        .arg(folder)
        .current_dir(dir.parent().unwrap())
        .status()?;
    if !status.success() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "zip command failed",
        )));
    }
    Ok(())
}
