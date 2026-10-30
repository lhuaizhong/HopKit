param([string]$Version='')
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$sourceVersion=(Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
if(!$Version){$Version=$sourceVersion}
if($Version -ne $sourceVersion){throw 'Package version must match package.json'}
$releaseRoot=Join-Path $projectRoot 'release'
$packageName="hopkit-$Version-windows-x64"
$packagePath=Join-Path $releaseRoot $packageName
if(Test-Path -LiteralPath $packagePath){throw "Output already exists: $packagePath. Use a new version/output or archive the previous package."}
New-Item -ItemType Directory -Path $packagePath | Out-Null
New-Item -ItemType Directory -Path (Join-Path $packagePath 'shortcuts'),(Join-Path $packagePath 'validation') | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/HopKit.exe') -Destination (Join-Path $packagePath 'HopKit.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/hopkit-cli.exe') -Destination (Join-Path $packagePath 'hopkit-cli.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/hopkit-cli.exe') -Destination (Join-Path $packagePath 'shortcuts/hk.exe')
foreach($file in @('LICENSE','README.md','README.zh-CN.md','README-macOS.md','MIGRATION.md','测试报告-HopKit命名迁移.md')){Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $packagePath}
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.zh-CN.md') -Destination (Join-Path $packagePath '使用说明.md')
& python (Join-Path $PSScriptRoot 'build-info.py') --target x86_64-pc-windows-msvc --build-dir (Join-Path $projectRoot 'target/release') --output-dir $packagePath
if($LASTEXITCODE -ne 0){throw 'Metadata generation failed'}

$cliHash=(Get-FileHash -LiteralPath (Join-Path $packagePath 'hopkit-cli.exe') -Algorithm SHA256).Hash.ToLower()
foreach($file in @('integration-release.json','gui.json')){
  $evidence=Join-Path $projectRoot "test-output/$file"
  if(Test-Path -LiteralPath $evidence){
    $report=Get-Content -LiteralPath $evidence -Raw | ConvertFrom-Json
    if($report.sha256 -eq $cliHash){Copy-Item -LiteralPath $evidence -Destination (Join-Path $packagePath "validation/$file")}
  }
}
$hashes=Get-ChildItem -LiteralPath $packagePath -File -Recurse | ForEach-Object{ $hash=Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256; $relative=$_.FullName.Substring($packagePath.Length+1).Replace('\','/'); "$($hash.Hash.ToLower())  $relative" }
$hashes | Set-Content -LiteralPath (Join-Path $packagePath 'SHA256SUMS.txt') -Encoding UTF8
Compress-Archive -LiteralPath $packagePath -DestinationPath (Join-Path $releaseRoot "$packageName.zip")
Get-Item -LiteralPath (Join-Path $releaseRoot "$packageName.zip") | Select-Object FullName,Length
Get-FileHash -LiteralPath (Join-Path $releaseRoot "$packageName.zip") -Algorithm SHA256

& python (Join-Path $PSScriptRoot 'verify-package.py') --archive (Join-Path $releaseRoot "$packageName.zip")
if($LASTEXITCODE -ne 0){throw 'Packaged CLI acceptance failed'}
