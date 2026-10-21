plugins {
    id("com.android.application")
}

// The benchmark's compose-rs app: examples/feed in a ComposeRsView, in a plain Activity (no
// AndroidX), the same screens as bench-compose's.
android {
    namespace = "rs.compose.bench.rs"
    compileSdk = 36
    defaultConfig {
        applicationId = "rs.compose.bench.rs"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1"
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    sourceSets["main"].jniLibs.directories.add("build/rustJniLibs")
}

val buildRust = tasks.register<Exec>("buildRust") {
    val output = layout.buildDirectory.dir("rustJniLibs").get().asFile
    commandLine(rootDir.resolve("build-rust.sh").path, "feed", "compose_rs_feed", output.path)
}
tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    implementation(project(":compose-rs"))
}
