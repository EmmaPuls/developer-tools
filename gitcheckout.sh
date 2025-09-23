#!/bin/bash

# If -b flag is used, just pass through to git checkout normally
if [[ "$1" == "-b" ]]; then
    git checkout "$@"
    exit $?
fi

# Extract the branch name (last argument that doesn't start with -)
local branch_name=""
for arg in "$@"; do
    if [[ "$arg" != -* ]]; then
        branch_name="$arg"
    fi
done

# Use Git Fetch to check if the branch exists on the remote
if git fetch origin "$branch_name" &>/dev/null; then
    # Remote branch exists, now check if we already have a local branch
    if git checkout "$branch_name" &>/dev/null; then
        echo "✅ Switched to existing local branch '$branch_name'"
    else
        # No local branch exists, create one tracking the remote
        echo "✅ Remote branch found. Creating local tracking branch..."
        if git checkout -b "$branch_name" "origin/$branch_name"; then
            echo "✅ Successfully created local branch tracking remote '$branch_name'"
        else
            echo "❌ Failed to create local tracking branch for '$branch_name'"
            exit 1
        fi
    fi
else
    # Branch doesn't exist on remote, create a new local branch
    echo "🆕 Branch '$branch_name' doesn't exist remotely. Creating new local branch..."
    git checkout -b "$branch_name"
fi