#!/bin/bash

# If -b flag is used, just pass through to git checkout normally
if [[ "$*" == *"-b"* ]]; then
    git checkout "$@"
    exit $?
fi

# Try to checkout the branch first
if git checkout "$@" 2>/dev/null; then
    echo "✅ Switched to existing branch"
    exit 0
else
    # If checkout failed, assume it's because the branch doesn't exist
    # Extract the branch name (last argument that doesn't start with -)
    local branch_name=""
    for arg in "$@"; do
        if [[ "$arg" != -* ]]; then
            branch_name="$arg"
        fi
    done
    
    if [[ -n "$branch_name" ]]; then
        echo "🆕 Branch '$branch_name' doesn't exist, creating new branch..."
        
        # Create and checkout new branch
        if git checkout -b "$branch_name"; then
            echo "✅ Created and switched to new branch '$branch_name'"
            
            # Set up remote tracking
            echo "🔗 Setting up remote tracking..."
            git push -u origin "$branch_name"
            
            if [[ $? -eq 0 ]]; then
                echo "✅ Remote tracking set up successfully"
            else
                echo "⚠️  Warning: Could not set up remote tracking (branch created locally)"
            fi
        else
            echo "❌ Failed to create branch '$branch_name'"
            exit 1
        fi
    else
        echo "❌ Could not determine branch name from arguments: $*"
        exit 1
    fi
fi
