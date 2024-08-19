# Structures and Routines for Reading and Writing WAV Cue Points and Metadata

Primarily intended for manipulating Quake sound effect loops, this library
can read and write WAV "cue " chunks as well as associated data stored in "LIST"
chunks.  This is a low-level library in the sense that it is the consumer's
responsibility to ensure that WAV files created with the library are compatible
with the requirements for software using the created files.

# Changelog

0.1.0:
* First release for public consumption
* Made public constants crate-private (shouldn't be needed by consumers)

# License

Triple-licensed under MIT / Apache 2.0 / CC0 (your choice)
