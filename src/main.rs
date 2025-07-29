use quick_xml::de::from_str;
use semver::Version;
use serde::Deserialize;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use zip::write::FileOptions;
use zip::CompressionMethod;

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

    let mut version = match parse_version(&old_version) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Invalid version '{old_version}': {e}");
            return Err(Box::new(e));
        }
    };
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

    let output_zip = release_dir.join(format!("{}.xrnx", tool_id));
    if output_zip.exists() {
        fs::remove_file(&output_zip)?;
    }
    zip_sources(manifest_path, &output_zip)?;
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
    let version = manifest
        .version
        .ok_or(ManifestError::MissingField("Version"))?;

    Ok((id, version))
}

fn parse_version(input: &str) -> Result<Version, semver::Error> {
    match Version::parse(input) {
        Ok(v) => Ok(v),
        Err(e) => {
            let (base, rest) = match input.find(|c| c == '-' || c == '+') {
                Some(idx) => (&input[..idx], Some(&input[idx..])),
                None => (input, None),
            };
            let count = base.split('.').filter(|s| !s.is_empty()).count();
            let adjusted = match count {
                1 => format!("{}.0.0", base.trim_end_matches('.')),
                2 => format!("{}.0", base.trim_end_matches('.')),
                _ => return Err(e),
            };
            let candidate = match rest {
                Some(r) => format!("{}{}", adjusted, r),
                None => adjusted,
            };
            Version::parse(&candidate)
        }
    }
}

fn zip_sources(manifest: &Path, out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in fs::read_dir(".")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "lua").unwrap_or(false) {
            let name = path.file_name().unwrap().to_str().unwrap();
            zip.start_file(name, options)?;
            let mut f = File::open(path)?;
            io::copy(&mut f, &mut zip)?;
        }
    }

    let readme_lower = Path::new("readme.md");
    let readme_upper = Path::new("README.md");
    if readme_lower.exists() || readme_upper.exists() {
        let path = if readme_lower.exists() { readme_lower } else { readme_upper };
        zip.start_file("README.md", options)?;
        let mut f = File::open(path)?;
        io::copy(&mut f, &mut zip)?;
    }

    zip.start_file("manifest.xml", options)?;
    let mut f = File::open(manifest)?;
    io::copy(&mut f, &mut zip)?;

    zip.finish()?;
    Ok(())
}
