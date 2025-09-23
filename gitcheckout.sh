#!/bin/bash

# If -b flag is used, just pass through to git checkout normally

# Extract the branch name (last argument that doesn't start with -)
branch_name=""
for arg in "$@"; do
    if [[ "$arg" != -* ]]; then
        branch_name="$arg"
    fi
done

# Try to checkout the branch first (existing local branch)
if git checkout "$branch_name" 2>/dev/null; then
    echo "✅ Switched to existing branch"
    exit 0
fi

# If that failed, try to create a local tracking branch from remote
if git checkout -b "$branch_name" "origin/$branch_name" 2>/dev/null; then
    echo "✅ Created local tracking branch from remote 'origin/$branch_name'"
    exit 0
fi

# If that failed, create a completely new branch
echo "🆕 Creating new local branch '$branch_name'..."
if git checkout -b "$branch_name"; then
    echo "✅ Created and switched to new branch '$branch_name'"
    
    # Set up remote tracking
    echo "🔗 Setting up remote tracking..."
    git push -u origin "$branch_name"
    
    if [[ $? -eq 0 ]]; then
        echo "✅ Remote tracking set up successfully"
    else
        echo "⚠️  Warning: Could not set up remote tracking (branch created locally)"
        git co master && git branch -D "$branch_name"
        echo "🗑️  Deleted local branch '$branch_name' due to remote setup failure"
    fi
else
    echo "❌ Failed to create branch '$branch_name'"
    exit 1
fi