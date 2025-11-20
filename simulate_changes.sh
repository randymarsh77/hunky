#!/usr/bin/env bash

set -e

# Configuration
TEST_REPO_URL="${TEST_REPO_URL:-https://github.com/ratatui/ratatui.git}"
NESTED_DIR="test-repo"
COMMITS_BEHIND=10
DELAY_BETWEEN_COMMITS=5  # seconds

echo "Git Stream Test Simulation"
echo "============================"
echo ""

# Clean up any existing test directory
if [ -d "$NESTED_DIR" ]; then
    echo "Cleaning up existing test directory..."
    rm -rf "$NESTED_DIR"
fi

# Clone the repository
echo "Cloning $TEST_REPO_URL into $NESTED_DIR..."
git clone "$TEST_REPO_URL" "$NESTED_DIR"
cd "$NESTED_DIR"

# Get the default branch
DEFAULT_BRANCH=$(git symbolic-ref refs/remotes/origin/HEAD | sed 's@^refs/remotes/origin/@@')
echo "Default branch: $DEFAULT_BRANCH"

# Get the commit hash from COMMITS_BEHIND commits ago
TARGET_COMMIT=$(git rev-parse "HEAD~$COMMITS_BEHIND")
echo "Target commit (${COMMITS_BEHIND} commits behind): $TARGET_COMMIT"

# Create and checkout a new branch at the target commit
BRANCH_NAME="test-stream-$(date +%s)"
echo "Creating branch $BRANCH_NAME at $TARGET_COMMIT..."
git checkout -b "$BRANCH_NAME" "$TARGET_COMMIT"

# Get the list of commits to replay
echo ""
echo "Getting list of commits to replay..."
mapfile -t COMMITS < <(git rev-list --reverse "${TARGET_COMMIT}..origin/${DEFAULT_BRANCH}" | head -n "$COMMITS_BEHIND")

echo "Will replay ${#COMMITS[@]} commits"
echo ""
echo "============================"
echo "Setup complete!"
echo ""
echo "Now you can:"
echo "  1. In another terminal, run: cargo run -- --repo $NESTED_DIR"
echo "  2. Press Enter here to start replaying commits..."
echo ""
read -r -p "Press Enter to start simulation..."

# Replay commits one by one
for i in "${!COMMITS[@]}"; do
    commit="${COMMITS[$i]}"
    commit_num=$((i + 1))
    
    echo ""
    echo "============================"
    echo "Replaying commit $commit_num/${#COMMITS[@]}: $commit"
    
    # Get commit info
    commit_msg=$(git log --format=%B -n 1 "$commit" | head -n 1)
    echo "Message: $commit_msg"
    
    # Get the diff from this commit and apply it to working directory
    if git show "$commit" --format= | git apply --reject --whitespace=fix; then
        echo "Changes applied to working directory (not staged or committed)"
        
        # Touch the modified files to ensure file system events fire
        for file in $(git diff --name-only); do
            touch "$file" 2>/dev/null || true
        done
        
        # Show what changed
        echo ""
        echo "Files changed:"
        git diff --name-only
        echo ""
        echo "Hunks visible in git diff:"
        git diff --stat
        
        echo ""
        echo "Waiting ${DELAY_BETWEEN_COMMITS}s before next commit..."
        sleep "$DELAY_BETWEEN_COMMITS"
    else
        echo "Warning: Could not apply commit cleanly"
        echo "Trying to apply what we can..."
        # Even with conflicts, some changes may have been applied
        if [ -n "$(git diff)" ]; then
            echo "Some changes were applied"
            echo ""
            echo "Waiting ${DELAY_BETWEEN_COMMITS}s before next commit..."
            sleep "$DELAY_BETWEEN_COMMITS"
        else
            echo "No changes applied, skipping..."
            continue
        fi
    fi
done

echo ""
echo "============================"
echo "Simulation complete!"
echo "All $COMMITS_BEHIND commits have been replayed."
echo ""
echo "The test repository is in: $NESTED_DIR"
echo "You can clean it up with: rm -rf $NESTED_DIR"
