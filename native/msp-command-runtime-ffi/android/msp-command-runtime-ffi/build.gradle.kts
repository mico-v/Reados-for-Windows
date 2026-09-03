import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.charset.StandardCharsets
import java.util.zip.ZipFile
import java.util.zip.ZipInputStream

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

val repositoryRoot = project.projectDir.resolve("../../../..")
val androidAbi = providers.gradleProperty("androidAbi").orElse("arm64-v8a").get()
require(androidAbi in setOf("arm64-v8a", "x86_64")) {
    "androidAbi must be arm64-v8a or x86_64"
}
val rustTarget = providers.gradleProperty("rustTarget").orElse(
    if (androidAbi == "x86_64") "x86_64-linux-android" else "aarch64-linux-android",
).get()
require(
    rustTarget == if (androidAbi == "x86_64") "x86_64-linux-android" else "aarch64-linux-android",
) {
    "rustTarget does not match androidAbi"
}
val defaultRustArtifact = repositoryRoot.resolve(
    "target/$rustTarget/release/libmsp_command_runtime_ffi.so",
)
val rustArtifact = file(
    providers.gradleProperty("rustFfiArtifact")
        .orElse(providers.environmentVariable("READOS_MSP_COMMAND_RUNTIME_FFI_ANDROID_SO"))
        .orElse(defaultRustArtifact.absolutePath)
        .get(),
)
val preparedRustLibDir = layout.buildDirectory.dir("generated/rust-jniLibs").get().asFile
val preparedRustArtifact = File(preparedRustLibDir, "$androidAbi/libmsp_command_runtime_ffi.so")

val prepareRustArtifact = tasks.register("prepareRustArtifact") {
    inputs.file(rustArtifact)
    outputs.file(preparedRustArtifact)
    doLast {
        if (!rustArtifact.isFile) {
            throw GradleException(
                "Missing the real Rust Android artifact at ${rustArtifact.absolutePath}. " +
                    "Build native/msp-command-runtime-ffi with " +
                    "--target $rustTarget --release, or pass " +
                    "-PrustFfiArtifact=<path to libmsp_command_runtime_ffi.so>.",
            )
        }
        preparedRustArtifact.parentFile.mkdirs()
        rustArtifact.copyTo(preparedRustArtifact, overwrite = true)
    }
}

val expectedRustExports = setOf(
    "msp_command_runtime_ffi_abi_version",
    "msp_command_runtime_ffi_version_data",
    "msp_command_runtime_ffi_runtime_create",
    "msp_command_runtime_ffi_runtime_free",
    "msp_command_runtime_ffi_workspace_create",
    "msp_command_runtime_ffi_workspace_free",
    "msp_command_runtime_ffi_workspace_put_file",
    "msp_command_runtime_ffi_execute_json",
    "msp_command_runtime_ffi_result_exit_code",
    "msp_command_runtime_ffi_result_stdout_data",
    "msp_command_runtime_ffi_result_stderr_data",
    "msp_command_runtime_ffi_result_diagnostic_data",
    "msp_command_runtime_ffi_result_free",
)

fun findLlvmNm(): File? {
    val explicit = providers.environmentVariable("READOS_ANDROID_LLVM_NM").orNull
    val candidates = buildList {
        if (explicit != null) add(file(explicit))
        val ndkHome = providers.environmentVariable("ANDROID_NDK_HOME").orNull
        if (ndkHome != null) {
            val toolchains = file(ndkHome).resolve("toolchains/llvm/prebuilt")
            add(toolchains.resolve("linux-x86_64/bin/llvm-nm"))
            add(toolchains.resolve("windows-x86_64/bin/llvm-nm.exe"))
            add(toolchains.resolve("darwin-x86_64/bin/llvm-nm"))
        }
        val sdkHome = providers.environmentVariable("ANDROID_SDK_ROOT")
            .orElse(providers.environmentVariable("ANDROID_HOME"))
            .orNull
        if (sdkHome != null) {
            val toolchains = file(sdkHome).resolve("ndk/27.2.12479018/toolchains/llvm/prebuilt")
            add(toolchains.resolve("linux-x86_64/bin/llvm-nm"))
            add(toolchains.resolve("windows-x86_64/bin/llvm-nm.exe"))
            add(toolchains.resolve("darwin-x86_64/bin/llvm-nm"))
        }
    }
    return candidates.firstOrNull { it.isFile }
}

val checkRustAbiExports = tasks.register("checkRustAbiExports") {
    dependsOn(prepareRustArtifact)
    doLast {
        val nm = findLlvmNm() ?: throw GradleException(
            "llvm-nm was not found. Install Android NDK 27.2.12479018 or set " +
                "READOS_ANDROID_LLVM_NM.",
        )
        val output = ByteArrayOutputStream()
        project.exec {
            commandLine(nm.absolutePath, "-D", "--defined-only", preparedRustArtifact.absolutePath)
            standardOutput = output
            errorOutput = output
            isIgnoreExitValue = false
        }
        val symbols = output.toString(StandardCharsets.UTF_8.name())
            .lineSequence()
            .map { it.trim().split(Regex("\\s+")).lastOrNull().orEmpty() }
            .filter { it.startsWith("msp_command_runtime_ffi_") }
            .toSet()
        if (symbols != expectedRustExports) {
            throw GradleException(
                "Rust Android ABI exports drifted. Expected ${expectedRustExports.sorted()}, " +
                    "found ${symbols.sorted()}.",
            )
        }
    }
}

val verifyReleaseAar = tasks.register("verifyReleaseAar") {
    dependsOn(tasks.named("assembleRelease"))
    doLast {
        val aar = layout.buildDirectory.file(
            "outputs/aar/msp-command-runtime-ffi-release.aar",
        ).get().asFile
        if (!aar.isFile) {
            throw GradleException("Release AAR was not produced: ${aar.absolutePath}")
        }

        val expectedEntries = setOf(
            "jni/$androidAbi/libmsp_command_runtime_ffi.so",
            "jni/$androidAbi/libmsp_command_runtime_ffi_jni.so",
            "classes.jar",
            "assets/android-tool-provider-v1.json",
            "assets/provenance.json",
            "assets/NOTICE",
        )
        val expectedClasses = setOf(
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiAbi.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiBridge.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiClient.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiNative.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiResultHandle.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiRuntimeHandle.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiSession.class",
            "com/reados/msp/commandruntime/MspCommandRuntimeFfiWorkspaceHandle.class",
            "com/reados/msp/commandruntime/MspFfiByteBuffer.class",
            "com/reados/msp/commandruntime/MspVirtualRequest.class",
            "com/reados/msp/commandruntime/MspVirtualResult.class",
            "com/reados/msp/commandruntime/MspAndroidToolProvider.class",
            "com/reados/msp/commandruntime/MspAndroidToolProviderFactory.class",
            "com/reados/msp/commandruntime/MspAndroidToolProfile.class",
            "com/reados/msp/commandruntime/MspAndroidToolProviderSource.class",
        )
        ZipFile(aar).use { archive ->
            val entries = archive.entries().asSequence().map { it.name }.toSet()
            val missingEntries = expectedEntries - entries
            if (missingEntries.isNotEmpty()) {
                throw GradleException("AAR is missing required entries: ${missingEntries.sorted()}")
            }

            val classesEntry = archive.getEntry("classes.jar")
                ?: throw GradleException("AAR is missing classes.jar")
            val classes = mutableSetOf<String>()
            ZipInputStream(archive.getInputStream(classesEntry)).use { classesArchive ->
                while (true) {
                    val entry = classesArchive.nextEntry ?: break
                    if (!entry.isDirectory) classes += entry.name
                }
            }
            val missingClasses = expectedClasses - classes
            if (missingClasses.isNotEmpty()) {
                throw GradleException("AAR is missing required Kotlin classes: ${missingClasses.sorted()}")
            }

            val temporaryDirectory = layout.buildDirectory.dir("tmp/verifyReleaseAar").get().asFile
            temporaryDirectory.deleteRecursively()
            temporaryDirectory.mkdirs()
            try {
                val rustLibrary = temporaryDirectory.resolve("libmsp_command_runtime_ffi.so")
                archive.getInputStream(
                    archive.getEntry("jni/$androidAbi/libmsp_command_runtime_ffi.so")!!,
                ).use { input -> rustLibrary.outputStream().use(input::copyTo) }
                val nm = findLlvmNm() ?: throw GradleException(
                    "llvm-nm was not found; install Android NDK 27.2.12479018 or set READOS_ANDROID_LLVM_NM.",
                )
                val output = ByteArrayOutputStream()
                project.exec {
                    commandLine(nm.absolutePath, "-D", "--defined-only", rustLibrary.absolutePath)
                    standardOutput = output
                    errorOutput = output
                }
                val symbols = output.toString(StandardCharsets.UTF_8.name())
                    .lineSequence()
                    .map { it.trim().split(Regex("\\s+")).lastOrNull().orEmpty() }
                    .filter { it.startsWith("msp_command_runtime_ffi_") }
                    .toSet()
                if (symbols != expectedRustExports) {
                    throw GradleException(
                        "AAR Rust ABI exports drifted. Expected ${expectedRustExports.sorted()}, " +
                            "found ${symbols.sorted()}.",
                    )
                }
            } finally {
                temporaryDirectory.deleteRecursively()
            }
        }
    }
}

tasks.matching { it.name == "preBuild" }.configureEach {
    dependsOn(checkRustAbiExports)
}

tasks.configureEach {
    if (name.startsWith("configureCMake") || name.contains("ExternalNativeBuild")) {
        dependsOn(checkRustAbiExports)
    }
}

android {
    namespace = "com.reados.msp.commandruntime"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        minSdk = 24
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
        externalNativeBuild {
            cmake {
                cppFlags += listOf("-std=c++17", "-fvisibility=hidden", "-Wall", "-Wextra", "-Werror")
                arguments += listOf(
                    "-DRUST_FFI_LIBRARY=${preparedRustArtifact.absolutePath.replace('\\', '/')}",
                    "-DRUST_FFI_INCLUDE_DIR=${repositoryRoot.resolve("native/msp-command-runtime-ffi/include").absolutePath.replace('\\', '/')}",
                )
            }
        }
        ndk {
            abiFilters += setOf(androidAbi)
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
        debug {
            isJniDebuggable = true
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    sourceSets["main"].java.srcDirs("../src/main/kotlin")
    sourceSets["main"].assets.srcDir("../provider")
    sourceSets["test"].java.srcDirs("../src/test/kotlin")
    sourceSets["androidTest"].java.srcDirs("../src/androidTest/kotlin")
    sourceSets["androidTest"].manifest.srcFile("../src/androidTest/AndroidManifest.xml")
    sourceSets["androidTest"].assets.srcDir(
        repositoryRoot.resolve("native/msp-command-pack/profile"),
    )
    sourceSets["main"].jniLibs.srcDir(preparedRustLibDir)

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    testImplementation(kotlin("test"))
    androidTestImplementation(kotlin("test"))
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
}
