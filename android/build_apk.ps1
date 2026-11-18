$ErrorActionPreference = "Stop"

$AndroidSdk = "C:\Users\Aminnur\AppData\Local\Android\Sdk"
$BuildTools = "$AndroidSdk\build-tools\35.0.0"
$AndroidJar = "$AndroidSdk\platforms\android-35\android.jar"

$Aapt2 = "$BuildTools\aapt2.exe"
$D8 = "$BuildTools\d8.bat"
$ZipAlign = "$BuildTools\zipalign.exe"
$ApkSigner = "$BuildTools\apksigner.bat"
$KeyTool = "keytool.exe"
$Javac = "javac.exe"
$Jar = "jar.exe"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$AppDir = "$ScriptDir\app\src\main"
$BuildDir = "$ScriptDir\build"

Write-Host "=== Building Kestrel Android APK ==="
Write-Host "App directory: $AppDir"
Write-Host "Build directory: $BuildDir"

if (Test-Path $BuildDir) {
    Remove-Item -Recurse -Force $BuildDir
}
New-Item -ItemType Directory -Force -Path "$BuildDir\compiled_res" | Out-Null
New-Item -ItemType Directory -Force -Path "$BuildDir\gen" | Out-Null
New-Item -ItemType Directory -Force -Path "$BuildDir\classes" | Out-Null
New-Item -ItemType Directory -Force -Path "$BuildDir\dex" | Out-Null

# Step 1: aapt2 compile
Write-Host "[1/7] Compiling Android resources with aapt2..."
& $Aapt2 compile --dir "$AppDir\res" -o "$BuildDir\resources.zip"
if ($LASTEXITCODE -ne 0) { throw "aapt2 compile failed" }

# Step 2: aapt2 link
Write-Host "[2/7] Linking resources and generating R.java..."
& $Aapt2 link -I $AndroidJar `
    --manifest "$AppDir\AndroidManifest.xml" `
    -o "$BuildDir\base.apk" `
    --java "$BuildDir\gen" `
    --auto-add-overlay `
    "$BuildDir\resources.zip"
if ($LASTEXITCODE -ne 0) { throw "aapt2 link failed" }

# Step 3: javac compile
Write-Host "[3/7] Compiling Java classes with javac (Java 17 target)..."
$JavaFiles = Get-ChildItem -Path "$BuildDir\gen", "$AppDir\java" -Filter "*.java" -Recurse | Select-Object -ExpandProperty FullName
& $Javac -encoding UTF-8 -source 17 -target 17 -cp $AndroidJar -d "$BuildDir\classes" $JavaFiles
if ($LASTEXITCODE -ne 0) { throw "javac failed" }

# Step 4: d8 compile classes to classes.dex
Write-Host "[4/7] Converting class files to DEX with d8..."
$ClassFiles = Get-ChildItem -Path "$BuildDir\classes" -Filter "*.class" -Recurse | Select-Object -ExpandProperty FullName
& $D8 --min-api 26 --output "$BuildDir\dex" $ClassFiles
if ($LASTEXITCODE -ne 0) { throw "d8 failed" }

# Step 5: Add DEX and native libraries to base.apk using jar
Write-Host "[5/7] Injecting classes.dex and native libraries into base.apk..."
Push-Location "$BuildDir\dex"
& $Jar -uf "$BuildDir\base.apk" classes.dex
Pop-Location
if ($LASTEXITCODE -ne 0) { throw "jar add classes.dex failed" }

# Prepare native libs directory structure for jar
New-Item -ItemType Directory -Force -Path "$BuildDir\native_libs\lib\arm64-v8a" | Out-Null
New-Item -ItemType Directory -Force -Path "$BuildDir\native_libs\lib\x86_64" | Out-Null
Copy-Item "$AppDir\jniLibs\arm64-v8a\libkestrel_mobile.so" "$BuildDir\native_libs\lib\arm64-v8a\libkestrel_mobile.so"
Copy-Item "$AppDir\jniLibs\x86_64\libkestrel_mobile.so" "$BuildDir\native_libs\lib\x86_64\libkestrel_mobile.so"

Push-Location "$BuildDir\native_libs"
& $Jar -uf "$BuildDir\base.apk" lib
Pop-Location
if ($LASTEXITCODE -ne 0) { throw "jar add native libs failed" }

# Step 6: zipalign 4KB
Write-Host "[6/7] 4KB page-aligning APK with zipalign..."
$AlignedApk = "$BuildDir\kestrel-aligned.apk"
& $ZipAlign -f -p 4 "$BuildDir\base.apk" $AlignedApk
if ($LASTEXITCODE -ne 0) { throw "zipalign failed" }

# Step 7: apksigner
Write-Host "[7/7] Signing APK with apksigner..."
$Keystore = "$ScriptDir\debug.keystore"
if (-not (Test-Path $Keystore)) {
    Write-Host "Creating debug keystore..."
    & $KeyTool -genkey -v -keystore $Keystore -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -storepass android -keypass android -dname "CN=Android Debug,O=Android,C=US"
}

$FinalApk = "$ScriptDir\kestrel.apk"
& $ApkSigner sign --ks $Keystore --ks-pass pass:android --key-pass pass:android --out $FinalApk $AlignedApk
if ($LASTEXITCODE -ne 0) { throw "apksigner failed" }

Write-Host "Verifying signature..."
& $ApkSigner verify --verbose $FinalApk

Write-Host "`n=== APK BUILD SUCCESSFUL ==="
$ApkInfo = Get-Item $FinalApk
Write-Host ("Output APK: {0} ({1:N2} MB)" -f $ApkInfo.FullName, ($ApkInfo.Length / 1MB))
