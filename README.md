# Dashtable: a concurrent raw hash table (fork of dashmap)

[![Minimum Rust 1.85.0](https://img.shields.io/badge/rust-1.85.0%2B-orange.svg?logo=rust)](https://releases.rs/docs/1.85.0/)
[![Lines of Code](https://www.aschey.tech/tokei/github/gendx/dashtable?category=code&branch=main)](https://github.com/gendx/dashtable)
[![Dependencies](https://deps.rs/repo/github/gendx/dashtable/status.svg)](https://deps.rs/repo/github/gendx/dashtable)
[![Codecov](https://codecov.io/gh/gendx/dashtable/branch/main/graph/badge.svg)](https://app.codecov.io/gh/gendx/dashtable/tree/main)
[![Build Status](https://github.com/gendx/dashtable/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/gendx/dashtable/actions/workflows/build.yml)
[![Test Status](https://github.com/gendx/dashtable/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/gendx/dashtable/actions/workflows/tests.yml)

This is a fork of the `dashmap` crate that adds minimal support for a raw
`HashTable` API akin to the `hashbrown` crate.
