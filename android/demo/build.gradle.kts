plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "rs.compose.demo"
    compileSdk = 37
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
        // Cargo's Android profiles (Cargo.toml, D361): both abort on panics; the release one
        // adds fat LTO and one codegen unit, which debuggable builds skip to rebuild faster.
        // By the build type's debuggable flag, so other release-like types take LTO too.
        val debuggable = variant.buildType?.let { android.buildTypes.getByName(it).isDebuggable } ?: true
        val cargoProfile = if (debuggable) "android" else "android-release"
        val buildRust = tasks.register<BuildRust>(name) {
            val output = outputDirectory
            executable = rootDir.resolve("build-rust.sh").path
            args("android-demo", "compose_rs_demo")
            argumentProviders.add(CommandLineArgumentProvider { listOf(output.get().asFile.path, cargoProfile) })
            environment("ANDROID_NDK_VERSION", android.ndkVersion)
            // What build-rust.sh reads, through providers: the configuration cache keeps the
            // environment the task was configured with, and is discarded when these change.
            // Unset ones are passed empty (the script reads empty as unset), so each is always
            // the value the cache entry was made with.
            val variables = listOf("ANDROID_NDK_HOME", "ANDROID_HOME", "ANDROID_SDK_ROOT", "HOME", "PATH")
            for (variable in variables) {
                environment(variable, providers.environmentVariable(variable).getOrElse(""))
            }
            outputs.upToDateWhen { false }
        }
        variant.sources.jniLibs?.addGeneratedSourceDirectory(buildRust, BuildRust::outputDirectory)
    }
}

dependencies {
    implementation(project(":compose-rs"))
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.13.0")
}
