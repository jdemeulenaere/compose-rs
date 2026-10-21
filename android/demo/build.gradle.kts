plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "rs.compose.demo"
    compileSdk = 36
    defaultConfig {
        applicationId = "rs.compose.demo"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
    }
    buildFeatures {
        compose = true
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            // Signed with the debug key: installable for trying and profiling.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    sourceSets["main"].jniLibs.directories.add("build/rustJniLibs")
}

// The examples' Rust library (examples/android-demo), built for arm64 by cargo.
val buildRust = tasks.register<Exec>("buildRust") {
    val output = layout.buildDirectory.dir("rustJniLibs").get().asFile
    commandLine(rootDir.resolve("build-rust.sh").path, "android-demo", "compose_rs_demo", output.path)
}
tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    implementation(project(":compose-rs"))
    implementation(platform("androidx.compose:compose-bom:2026.06.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.12.4")
}
