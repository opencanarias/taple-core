# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2023-08-03

### Added

- Display trait for Identificators 

### Fixed

- Distribution manager "Not Found" panic

## [0.2.0] - 2023-07-26

### Added

- Smart contracts. Now the modifications of the state of the subjects is done through smart contracts. This allows for more advanced control over which parts of the state can be modified and who can make such modifications.
- New event types: Transfer and EOL.
- New validation process. The validation process is now managed by the owner, reducing the network load and improving efficiency.
- Namespace segmentation. Using namespaces we can segment the participants of the use case in any of the phases of an event.
- Preauthorized subjects and providers API

### Changed

- Database abstraction. Now the database implementation is not core-dependent
- Crate name changed to taple-core
- Approval API is now more usable

### Fixed

- Several bugs fixed and improvements.

### Removed

- LevelDB database implementation moved to taple-client

## [0.1.2] - 2023-02-22

### Added

- Adding deny.toml to check licence problems (#4)

### Changed

- Pagination in GET Governances & GET Subjects
- Dependencies moved to workspace cargo.toml (#6)

### Fixed

- GetEvents Pagination error
- Maximum limit of events that can be obtained with GetEvents
- Node shutdown if subject requested is not found & error management improvement
- Temporary directory is only created when needed (#5)
- Retry communication with not available bootnodes
- LevelDB tests adjusted

## [0.1.1] - 2023-02-17

### Added

- Community files added

### Fixed

- Governance quorum is not possible to validate

## [0.1.0] - 2022-11-30

### Added

- First release

[0.2.1]: https://github.com/opencanarias/taple-core/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/opencanarias/taple-core/compare/v0.1.0...v0.2.0
[0.1.2]: https://github.com/opencanarias/taple-core/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/opencanarias/taple-core/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/opencanarias/taple-core/releases/tag/v0.1.0
