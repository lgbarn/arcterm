#!/bin/bash
set -x
name="$1"

notes=$(cat <<EOT
See TODO(arcterm): replace with ArcTerm changelog URL for the changelog

If you're looking for nightly downloads or more detailed installation instructions:

[Windows](TODO(arcterm): replace with ArcTerm Windows install URL)
[macOS](TODO(arcterm): replace with ArcTerm macOS install URL)
[Linux](TODO(arcterm): replace with ArcTerm Linux install URL)
[FreeBSD](TODO(arcterm): replace with ArcTerm FreeBSD install URL)
EOT
)

gh release view "$name" || gh release create --prerelease --notes "$notes" --title "$name" "$name"
