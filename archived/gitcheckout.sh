#!/bin/bash

# Extract the branch name (last argument that doesn't start with -)
branch_name=""
for arg in "$@"; do
    if [[ "$arg" != -* ]]; then
        branch_name="$arg"
    fi
done
echo "🔍 Branch name: '$branch_name'"

# Try doing git checkout, pass all args and variables as is
echo "🔍 Trying git checkout..."
if git checkout "$@" &>/dev/null; then
    echo "✅ Switched to branch '$branch_name'"
    exit $?
else
    echo "❌ Failed to switch to branch '$branch_name'"
fi


# Use Git Fetch to check if the branch exists on the remote
if git ls-remote --heads origin "$branch_name" &>/dev/null; then
    # Remote branch exists, now check if we already have a local branch
    if git checkout "$branch_name" &>/dev/null; then
        echo "✅ Switched to existing local branch '$branch_name'"
    else
        # No local branch exists, create one tracking the remote
        echo "✅ Remote branch found. Creating local tracking branch..."
        if git checkout -b "$branch_name" "origin/$branch_name"; then
            echo "✅ Successfully created local branch tracking remote '$branch_name'"
        else
            git checkout -b "$branch_name"
            if git po "$branch_name" &>/dev/null; then
                echo "✅ Created and switched to new local branch '$branch_name'"
            else
                git co master && git branch -D "$branch_name" && git checkout "$branch_name"
            fi
        fi
    fi
else
    # Branch doesn't exist on remote, create a new local branch
    echo "🆕 Branch '$branch_name' doesn't exist remotely. Creating new local branch..."
    git checkout -b "$branch_name"
fi