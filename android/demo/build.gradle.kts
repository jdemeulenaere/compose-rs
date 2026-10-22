plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "rs.compose.demo"
    compileSdk = 36
    // Pinned (README.md): the Android Gradle plugin strips native libraries only when it's set.
    ndkVersion = "27.2.12479018"
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
}

// Builds the examples' Rust library (examples/android-demo) for arm64 with build-rust.sh.
// Each variant's jniLibs take the task's output directory, so Gradle runs it before merging the
// native libraries and packages the library it built (an Exec task without declared outputs,
// run before preBuild, sometimes left the previous build's library in the APK).
// Never up to date: cargo decides what to rebuild.
abstract class BuildRust : Exec() {
    @get:OutputDirectory
    abstract val outputDirectory: DirectoryProperty
}

androidComponents {
    onVariants { variant ->
        val name = "buildRust" + variant.name.replaceFirstChar(Char::uppercase)
        val buildRust = tasks.register<BuildRust>(name) {
            val output = outputDirectory
            executable = rootDir.resolve("build-rust.sh").path
            args("android-demo", "compose_rs_demo")
            argumentProviders.add(CommandLineArgumentProvider { listOf(output.get().asFile.path) })
            environment("ANDROID_NDK_VERSION", android.ndkVersion)
            outputs.upToDateWhen { false }
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(buildRust, BuildRust::outputDirectory)
    }
}

dependencies {
    implementation(project(":compose-rs"))
    implementation(platform("androidx.compose:compose-bom:2026.06.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.12.4")
}
