pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "compose-rs-android"
// The library: ComposeRsView and its Kotlin glue (an AAR).
include(":compose-rs")
// The examples (counter, todo, gallery) in a Kotlin Compose app.
include(":demo")
