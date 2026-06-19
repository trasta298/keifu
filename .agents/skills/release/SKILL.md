---
name: release
description: Create a new release with tag, GitHub release, and homebrew-tap update
---

# Release Skill

This skill creates a new release by:
1. Bumping the version in Cargo.toml
2. Creating a git commit and tag
3. Pushing to trigger GitHub Actions release workflow
4. Waiting for the release to complete
5. Updating the homebrew-tap formula

## Instructions

### Step 1: Analyze Changes and Determine Recommended Version

First, get the current version from Cargo.toml and analyze changes since the last release:

```bash
# Get current version
grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/'

# Get the latest tag
git describe --tags --abbrev=0 2>/dev/null || echo "No tags found"

# Get commits since last tag (or all commits if no tag)
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null)
if [ -n "$LAST_TAG" ]; then
  git log --oneline "$LAST_TAG"..HEAD
else
  git log --oneline
fi
```

Determine the recommended version bump based on conventional commits:
- **major**: Breaking changes (commits with `BREAKING CHANGE` or `!:` in message)
- **minor**: New features (commits starting with `feat:` or similar)
- **patch**: Bug fixes, documentation, refactoring, etc. (commits starting with `fix:`, `docs:`, `refactor:`, `chore:`, etc.)

### Step 2: Ask User for Version Bump Type

Use the AskUserQuestion tool to ask the user which version bump to apply. **Put the recommended option first** based on the analysis above.

Question format:
- Header: "Version"
- Question: "Which version bump do you want to apply? Current version: X.Y.Z"
- Options should be ordered with recommended first, e.g., if patch is recommended:
  1. "patch (Recommended)" -> "X.Y.Z+1 - Bug fixes and minor changes"
  2. "minor" -> "X.Y+1.0 - New features"
  3. "major" -> "X+1.0.0 - Breaking changes"

### Step 3: Update Version and Create Release

After user confirms the version bump:

1. **Update Cargo.toml version**:
   - Calculate the new version based on user selection
   - Edit the version line in Cargo.toml

2. **Update Cargo.lock**:
   ```bash
   cargo update -p keifu
   ```

3. **Commit the version bump**:
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "$(cat <<'EOF'
   Bump version to NEW_VERSION

   Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
   EOF
   )"
   ```

4. **Create and push the tag**:
   ```bash
   git tag vNEW_VERSION
   git push origin main --tags
   ```

### Step 4: Wait for GitHub Actions Release Workflow

The release workflow is triggered by the tag push. Wait for it to complete:

```bash
# Wait for the workflow to start and complete
gh run list --workflow=release.yaml --limit=1 --json databaseId,status,conclusion

# Watch the workflow progress
gh run watch $(gh run list --workflow=release.yaml --limit=1 --json databaseId --jq '.[0].databaseId')
```

If the workflow fails, inform the user and stop. Do not proceed with homebrew-tap update.

### Step 5: Update Homebrew Tap

After the release workflow completes successfully:

1. **Create a temporary directory and clone homebrew-tap**:
   ```bash
   TEMP_DIR=$(mktemp -d)
   git clone git@github.com:trasta298/homebrew-tap.git "$TEMP_DIR"
   ```

2. **Download release assets and calculate SHA256**:
   ```bash
   NEW_VERSION="X.Y.Z"  # The new version without 'v' prefix
   TAG="vX.Y.Z"  # The tag with 'v' prefix

   # Get SHA256 for each platform
   TARGETS=(
     "aarch64-apple-darwin"
     "x86_64-apple-darwin"
     "aarch64-unknown-linux-gnu"
     "x86_64-unknown-linux-gnu"
   )

   for TARGET in "${TARGETS[@]}"; do
     URL="https://github.com/trasta298/keifu/releases/download/$TAG/keifu-$TAG-$TARGET.tar.gz"
     SHA=$(curl -sL "$URL" | sha256sum | cut -d' ' -f1)
     echo "$TARGET: $SHA"
   done
   ```

3. **Update the formula file** (`$TEMP_DIR/Formula/keifu.rb`):
   - Update the `version` line
   - Update all URLs to use the new tag
   - Update all `sha256` values with the calculated hashes

4. **Commit and push the changes**:
   ```bash
   cd "$TEMP_DIR"
   git add Formula/keifu.rb
   git commit -m "$(cat <<'EOF'
   Update keifu to NEW_VERSION

   Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
   EOF
   )"
   git push origin main
   ```

5. **Clean up the temporary directory**:
   ```bash
   rm -rf "$TEMP_DIR"
   ```

### Step 6: Report Success

Inform the user that the release is complete with:
- The new version number
- Link to the GitHub release
- Confirmation that homebrew-tap was updated

## Error Handling

- If there are uncommitted changes, warn the user and ask if they want to proceed
- If the GitHub Actions workflow fails, stop and report the error
- If homebrew-tap update fails, report the error but note that the main release was successful
