param([string]$Version='0.1.0-alpha.1')
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$releaseRoot=Join-Path $projectRoot 'release'
$packageName="netbox-$Version-windows-x64"
$packagePath=Join-Path $releaseRoot $packageName
if(Test-Path -LiteralPath $packagePath){throw "Output already exists: $packagePath. Use a new version/output or archive the previous package."}
New-Item -ItemType Directory -Path $packagePath | Out-Null
New-Item -ItemType Directory -Path (Join-Path $packagePath 'shortcuts'),(Join-Path $packagePath 'validation') | Out-Null
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/netbox-desktop.exe') -Destination (Join-Path $packagePath 'netbox.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/netbox-cli.exe') -Destination (Join-Path $packagePath 'netbox-cli.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/netbox-cli.exe') -Destination (Join-Path $packagePath 'shortcuts/nb.exe')
foreach($file in @('LICENSE','README.md','README.zh-CN.md','README-macOS.md')){Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $packagePath}
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.zh-CN.md') -Destination (Join-Path $packagePath '使用说明.md')
foreach($file in @('测试报告.md','THIRD_PARTY_NOTICES.md','BUILD_INFO.json','SBOM.cdx.json')){if(Test-Path -LiteralPath (Join-Path $projectRoot $file)){Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $packagePath}}
if(Test-Path -LiteralPath (Join-Path $projectRoot 'licenses')){Copy-Item -LiteralPath (Join-Path $projectRoot 'licenses') -Destination $packagePath -Recurse}
foreach($file in @('integration-release.json','gui.json','unit-tests.txt')){if(Test-Path -LiteralPath (Join-Path $projectRoot "test-output/$file")){Copy-Item -LiteralPath (Join-Path $projectRoot "test-output/$file") -Destination (Join-Path $packagePath "validation/$file")}}
if(Test-Path -LiteralPath (Join-Path $projectRoot 'test-output/appearance/report.json')){Copy-Item -LiteralPath (Join-Path $projectRoot 'test-output/appearance/report.json') -Destination (Join-Path $packagePath 'validation/appearance.json')}
$hashes=Get-ChildItem -LiteralPath $packagePath -File -Recurse | ForEach-Object{ $hash=Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256; $relative=$_.FullName.Substring($packagePath.Length+1).Replace('\','/'); "$($hash.Hash.ToLower())  $relative" }
$hashes | Set-Content -LiteralPath (Join-Path $packagePath 'SHA256SUMS.txt') -Encoding UTF8
Compress-Archive -LiteralPath $packagePath -DestinationPath (Join-Path $releaseRoot "$packageName.zip")
Get-Item -LiteralPath (Join-Path $releaseRoot "$packageName.zip") | Select-Object FullName,Length
Get-FileHash -LiteralPath (Join-Path $releaseRoot "$packageName.zip") -Algorithm SHA256
