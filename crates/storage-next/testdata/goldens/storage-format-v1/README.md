# Storage Format V1 Goldens

This directory is the checked-in home for storage format V1 golden vectors.

No durable byte formats are frozen in the current scaffold. The first durable
format implementation must add fixtures here with metadata that records format
name, version, codec, checksum, and purpose. Normal tests must verify checked-in
goldens and must not rewrite them.
