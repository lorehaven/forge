#!/bin/sh
# Makes the checkout of a conveyor job fit to release from. The job's checkout is a shallow clone at one commit with
# no tags and no branch, and its git credential is the checkout container's, not this one's. A release needs the history
# and the tags (to see what changed since each package's last tag), a branch to commit on and push, and a credential
# that may push: the conveyor secret RELEASE_GIT_TOKEN, a token with write access to the repository.
#
#   RELEASE_GIT_TOKEN   required: the token (sent as the password of an https remote)
#   RELEASE_BRANCH      the branch released from, default master
#   RELEASE_REQUIRE_SHA 1 (default): the branch must still be at the commit this run was started for, so that what was
#                       tested is what is released; 0: take the branch as it is now (a second release job of the same
#                       run, which must see what the first one pushed)
#
# The same script is in quench and forge (scripts/ci-release-prepare.sh).
set -eu

branch="${RELEASE_BRANCH:-master}"
: "${RELEASE_GIT_TOKEN:?the conveyor secret RELEASE_GIT_TOKEN is not set: a release pushes, and needs a token that may}"

url=$(git config --get remote.origin.url)
case "$url" in
  http://* | https://*) ;;
  *)
    echo "origin is $url: a release job pushes over https with RELEASE_GIT_TOKEN, so register the repository with an https clone url" >&2
    exit 1
    ;;
esac
host=$(printf '%s' "$url" | sed -E 's#^(https?://[^/]+/).*#\1#')
auth=$(printf 'x-access-token:%s' "$RELEASE_GIT_TOKEN" | base64 -w0)
git config --global "http.${host}.extraheader" "Authorization: Basic ${auth}"

# The history and the tags. A clone that is already complete has nothing to unshallow.
git fetch --quiet --tags --unshallow origin 2>/dev/null || git fetch --quiet --tags origin
git fetch --quiet origin "$branch"

head=$(git rev-parse "origin/$branch")
if [ "${RELEASE_REQUIRE_SHA:-1}" = 1 ] && [ -n "${CONVEYOR_SHA:-}" ] && [ "$head" != "$CONVEYOR_SHA" ]; then
  echo "$branch is at $head, but this run was started for $CONVEYOR_SHA: something was pushed meanwhile. Start the release again." >&2
  exit 1
fi

git checkout --quiet -B "$branch" "origin/$branch"
git branch --quiet --set-upstream-to="origin/$branch" "$branch"
echo "ready to release from $branch at $(git rev-parse --short HEAD)"
git status --short
