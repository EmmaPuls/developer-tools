#!/bin/bash

## Interactive git squash
# ANSI escape codes for bold text
bold=$(tput bold)
normal=$(tput sgr0)
# The first variable is the number of commits to squash
count=$1
shift

# Initialise other variables
has_message=false
message=""
commit_options=""

# Parse the rest of the arguments
# If -m is present, store the message and remove it from the arguments
while [ $# -gt 0 ]; do
  case $1 in
    -m) has_message=true; message="$2"; shift ;; 
    *) commit_options="$commit_options $1" ;;
  esac
  shift
done

echo "${bold}You are about to squash the last $count commits.${normal}"
echo ""
# Capture recent commit messages into a variable
recent_commits=$(git log -n $count --pretty=format:"%h - %an: %s")
echo "${bold}Commit messages:${normal}"
echo "$recent_commits"
echo ""

if [ -n "$commit_options" ]; then
  echo "${bold}Additional commit options:${normal} $commit_options"
fi

# Get the earliest commit message from the range to be squashed
earliest_commit_message=$(git log --reverse -n $count --pretty=format:"%s" | head -n 1)

if [ "$has_message" = true ]; then
  echo "${bold}Commit message:${normal} $message"
else
  echo "${bold}Default commit message: ${normal}$earliest_commit_message"
  echo -n "${bold}Press Enter to use the default message or type a new one: ${normal}"
  read user_message
  if [ -n "$user_message" ]; then
    message="$user_message"
  else
    message="$earliest_commit_message"
  fi
  has_message=true
fi

# Finish the squash
echo ""
echo "${bold}Squashing commits...${normal}"
git reset --soft HEAD~$count

# Check if git reset was successful
if [ $? -ne 0 ]; then
  echo "${bold}Error: git reset failed.${normal}"
  return 1
else
  echo "${bold}Reset successful.${normal}"
fi

eval "git commit -m \"$message\" $commit_options"

# Check if git commit was successful
if [ $? -ne 0 ]; then
  echo "${bold}Error: git commit failed.${normal}"
  echo ""
  echo "${bold}Resetting to previous state...${normal}"

  # Reset to the previous state
  git reset --hard 'HEAD@{1}'
  return 1
else
  echo ""
  echo "${bold}Squash complete!${normal}"
  echo "Latest commit message: $message"
fi