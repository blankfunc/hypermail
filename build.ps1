$releasePath = "target/release/hypermail.dll"
$debugPath = "target/debug/hypermail.dll"

$mode = ""
if (Test-Path $releasePath) {
	$mode = "release"
	Write-Host "Found $releasePath"
} elseif (Test-Path $debugPath) {
	$mode = "debug"
	Write-Host "Found $debugPath"
} else {
	Write-Host "No DLL found, building in release mode..."
	cargo build -r
	$mode = "release"
}

foreach ($lang in @("kotlin", "swift", "python", "ruby")) {
	$dllPath = "target/$mode/hypermail.dll"
	$outDir = "bindgen/$lang"

	Write-Host "Generating bindings for $lang..."
	cargo run -r --bin uniffi-bindgen generate --library $dllPath --language $lang --out-dir $outDir
}