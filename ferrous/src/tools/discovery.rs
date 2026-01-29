use anyhow::Result;
use std::path::Path;
use std::collections::HashSet;

pub fn discover_technologies(cwd: &Path) -> Result<String> {
    let mut techs = HashSet::new();

    // Languages and Frameworks detection
    if cwd.join("Cargo.toml").exists() {
        techs.insert("Rust (Cargo)");
    }
    if cwd.join("package.json").exists() {
        techs.insert("Node.js/JavaScript/TypeScript");
    }
    if cwd.join("requirements.txt").exists() || cwd.join("pyproject.toml").exists() || cwd.join("setup.py").exists() {
        techs.insert("Python");
    }
    if cwd.join("go.mod").exists() {
        techs.insert("Go");
    }
    if cwd.join("pom.xml").exists() {
        techs.insert("Java (Maven)");
    }
    if cwd.join("build.gradle").exists() || cwd.join("build.gradle.kts").exists() {
        techs.insert("Java/Kotlin (Gradle)");
    }
    if cwd.join("Gemfile").exists() {
        techs.insert("Ruby");
    }
    if cwd.join("composer.json").exists() {
        techs.insert("PHP");
    }
    if cwd.join("CMakeLists.txt").exists() {
        techs.insert("C/C++ (CMake)");
    }
    if cwd.join("Makefile").exists() {
        techs.insert("Make");
    }
    if cwd.join("Dockerfile").exists() {
        techs.insert("Docker");
    }
    if cwd.join(".git").exists() {
        techs.insert("Git");
    }

    if techs.is_empty() {
        Ok("No specific technologies detected in the project root. Please use 'list_files_recursive' to explore the project structure.".to_string())
    } else {
        let mut tech_list: Vec<_> = techs.into_iter().collect();
        tech_list.sort();
        Ok(format!("Detected technologies: {}", tech_list.join(", ")))
    }
}
