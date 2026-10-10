$ErrorActionPreference = 'Stop'
# Native command failures are detected from $LASTEXITCODE below, so do
# not let the preference variable turn a non-zero exit into a throw
# before that status is captured.
$PSNativeCommandUseErrorActionPreference = $false

$metadataRaw = & cargo metadata --locked --no-deps --format-version 1
$metadataExit = $LASTEXITCODE
if ($metadataExit -ne 0) {
  # ::error:: prefix is gh actions workflow notation to
  # present a message as an error
  Write-Host "::error::cargo metadata failed with exit code $metadataExit"
  exit $metadataExit
}

# attempt to capture output metadata json > `$metadata`
try {
  $metadata = ($metadataRaw -join "`n") | ConvertFrom-Json -ErrorAction Stop
} catch {
  Write-Host "::error::failed to parse cargo metadata JSON: $_"
  exit 1
}

# find scarab's package within $metadata
#
# the repository package is the sole default workspace member.
# do not assume packages[0] or resolve.root, which is null in this metadata.
if ($null -eq $metadata.workspace_default_members) {
  Write-Host "::error::cargo metadata did not report workspace_default_members"
  exit 1
}
$defaultMembers = @($metadata.workspace_default_members)
if ($defaultMembers.Count -ne 1) {
  Write-Host "::error::expected exactly one default workspace member, found $($defaultMembers.Count)"
  exit 1
}
$repositoryPackageId = $defaultMembers[0]
$repositoryPackage = @(
  $metadata.packages | Where-Object { $_.id -eq $repositoryPackageId }
)
if ($repositoryPackage.Count -ne 1) {
  Write-Host "::error::could not identify a single repository package for default member '$repositoryPackageId'"
  exit 1
}
# found scarab package object, save to `$package`
$package = $repositoryPackage[0]
Write-Host "repository package: $($package.name) ($($package.id))"

# get targets exposed as library modules
$libraryTargets = @(
  $package.targets | Where-Object { $_.kind -contains 'lib' }
)
if ($libraryTargets.Count -ne 1) {
  Write-Host "::error::expected exactly one library target, found $($libraryTargets.Count)"
  exit 1
}
$libraryTarget = $libraryTargets[0]

# get targets exposed as integration test modules
$integrationTargets = @(
  $package.targets | Where-Object { $_.kind -contains 'test' }
)
$integrationNames = @($integrationTargets | ForEach-Object { $_.name })
$duplicateNames = @(
  $integrationNames |
    Group-Object -CaseSensitive |
    Where-Object { $_.Count -gt 1 } |
    ForEach-Object { $_.Name }
)
if ($duplicateNames.Count -gt 0) {
  Write-Host "::error::ambiguous integration target names: $($duplicateNames -join ', ')"
  exit 1
}
$invalidNames = @(
  $integrationNames | Where-Object { [string]::IsNullOrEmpty($_) }
)
if ($invalidNames.Count -gt 0) {
  Write-Host "::error::invalid empty integration target name in metadata"
  exit 1
}

# exclude only the integration `probe`, whose tests invoke the external ffprobe binary
# the library `probe` tests do not invoke ffprobe
$excludedNames = @($integrationNames | Where-Object { $_ -ceq 'probe' })
$selectedNames = @(
  $integrationNames | Where-Object { $_ -cne 'probe' } | Sort-Object
)

Write-Host "library target: $($libraryTarget.name) (whole target, no positional test-name filter)"
Write-Host "selected integration targets ($($selectedNames.Count)): $($selectedNames -join ', ')"
if ($excludedNames.Count -gt 0) {
  Write-Host "excluded integration target(s): $($excludedNames -join ', ') (requires external ffprobe)"
} else {
  Write-Host "excluded integration target(s): none (no target named 'probe' discovered)"
}

# construct `cargo test` command
$cargoArguments = @('test', '--locked', '--lib')
foreach ($name in $selectedNames) {
  $cargoArguments += @('--test', $name)
}
$expectedArgumentCount = 3 + (2 * $selectedNames.Count)
if ($cargoArguments.Count -ne $expectedArgumentCount) {
  Write-Host "::error::argument construction produced $($cargoArguments.Count) arguments, expected $expectedArgumentCount"
  exit 1
}
Write-Host "Invoking: cargo $($cargoArguments -join ' ')"

& cargo @cargoArguments
$testExit = $LASTEXITCODE
if ($testExit -ne 0) {
  Write-Host "::error::cargo test failed with exit code $testExit"
  exit $testExit
}
