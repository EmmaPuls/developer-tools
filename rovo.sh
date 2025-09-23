#!/bin/bash
echo "Rovo CLI Wrapper v1.0.0"
case "$1" in
    login)
        echo "Logging into Rovo..."
        if command acli rovodev auth login; then
            echo "Launching rovodev..."
            command acli rovodev run
        else
            exit 1
        fi
        ;;
    *)
        # Fall back to original rovo command if it exists
        if command -v rovo >/dev/null 2>&1; then
            command acli rovodev "$@"
        else
            echo "Usage: rovo [login|...]"
            echo "Available commands:"
            echo "login - Log into Rovo"
        fi
        ;;
esac