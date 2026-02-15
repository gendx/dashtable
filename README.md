# Dashtable: a concurrent raw hash table (fork of dashmap)

[![Crate](https://img.shields.io/crates/v/dashtable.svg?logo=rust)](https://crates.io/crates/dashtable)
[![Documentation](https://img.shields.io/docsrs/dashtable?logo=rust)](https://docs.rs/dashtable)
[![Minimum Rust 1.85.0](https://img.shields.io/badge/rust-1.85.0%2B-orange.svg?logo=rust)](https://releases.rs/docs/1.85.0/)
[![Lines of Code](https://www.aschey.tech/tokei/github/gendx/dashtable?category=code&branch=main)](https://github.com/gendx/dashtable)
[![Dependencies](https://deps.rs/repo/github/gendx/dashtable/status.svg)](https://deps.rs/repo/github/gendx/dashtable)
[![License](https://img.shields.io/crates/l/dashtable.svg)](https://github.com/gendx/dashtable/blob/0.1.0/LICENSE)
[![Codecov](https://codecov.io/gh/gendx/dashtable/branch/main/graph/badge.svg)](https://app.codecov.io/gh/gendx/dashtable/tree/main)
[![Build Status](https://github.com/gendx/dashtable/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/gendx/dashtable/actions/workflows/build.yml)
[![Test Status](https://github.com/gendx/dashtable/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/gendx/dashtable/actions/workflows/tests.yml)

This is a fork of the `dashmap` crate that adds minimal support for a raw
`HashTable` API akin to the `hashbrown` crate.
