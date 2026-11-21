// Adapted from Horizon Android (MIT). Copyright (c) 2026 Ludovic Marie and the Horizon contributors.
// See THIRD_PARTY_NOTICES.md and LICENSE.
use crate::Result;
use std::{fs, path::Path};
pub fn fetch(root: &Path, progress: impl Fn(u64, u64)) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../packaging/android/bedrock-controls.json"
    ))?;
    let revision = manifest["revision"]
        .as_str()
        .ok_or("缺少按键修订信息")?;
    let target = root.join("assets/minecraft/textures/gui/bedrock");
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();
    let entries = manifest["files"]
        .as_array()
        .ok_or("缺少按键文件")?;
    progress(0, entries.len() as u64);
    for (index, entry) in entries.iter().enumerate() {
        let name = entry["path"].as_str().ok_or("缺少按键路径")?;
        let hash = entry["sha256"].as_str().ok_or("缺少按键校验和")?;
        let size = entry["size"]
            .as_u64()
            .filter(|n| *n <= 1024 * 1024)
            .ok_or("无效的按键尺寸")?;
        let relative = Path::new(name);
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("无效的按键目标".into());
        }
        let file = target.join(relative);
        if fs::read(&file).is_ok_and(|data| {
            data.len() as u64 == size && format!("{:x}", Sha256::digest(data)) == hash
        }) {
            progress(index as u64 + 1, entries.len() as u64);
            continue;
        }
        let source = if name == "LICENSE.md" {
            "LICENSE.md".into()
        } else {
            format!(
                "resource_pack/textures/{}",
                entry["source"].as_str().ok_or("缺少按键来源")?
            )
        };
        let url =
            format!("https://raw.githubusercontent.com/Mojang/bedrock-samples/{revision}/{source}");
        let mut bytes = Vec::new();
        agent
            .get(&url)
            .call()?
            .into_reader()
            .take(size + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash {
            return Err(format!("按键贴图校验和不匹配：{name}").into());
        }
        fs::create_dir_all(file.parent().ok_or("缺少贴图父级")?)?;
        fs::write(file, bytes)?;
        progress(index as u64 + 1, entries.len() as u64);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "downloads the pinned control textures from Mojang"]
    fn fresh_resources_receive_the_complete_verified_overlay() {
        let root =
            std::env::temp_dir().join(format!("starling-fresh-controls-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fetch(&root, |_, _| {}).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../packaging/android/bedrock-controls.json"
        ))
        .unwrap();
        for entry in manifest["files"].as_array().unwrap() {
            let path = root
                .join("assets/minecraft/textures/gui/bedrock")
                .join(entry["path"].as_str().unwrap());
            assert_eq!(
                fs::metadata(path).unwrap().len(),
                entry["size"].as_u64().unwrap()
            );
        }
        // A restart uses the already verified local files.
        fetch(&root, |_, _| {}).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
