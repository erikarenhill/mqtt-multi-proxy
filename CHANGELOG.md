## [1.5.0](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.4.1...v1.5.0) (2026-02-09)


### Features

* try improve visual stability in web-ui ([502b40a](https://github.com/erikarenhill/mqtt-multi-proxy/commit/502b40aadd3c1e185bf5428e4cbacb3004dce7a8))

## [1.4.1](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.4.0...v1.4.1) (2026-02-07)


### Bug Fixes

* copy cargo.toml for access to version ([a47bbaa](https://github.com/erikarenhill/mqtt-multi-proxy/commit/a47bbaa5a500e5c5add6187df72248088703c371))

## [1.4.0](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.3.0...v1.4.0) (2026-02-07)


### Features

* add version to web-ui to easier see what version you're at ([c21cae3](https://github.com/erikarenhill/mqtt-multi-proxy/commit/c21cae3c4e48ec1377ce78fbef62178e095c433c))

## [1.3.0](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.2.0...v1.3.0) (2026-02-07)


### Features

* add support to configure main broker from webui instead of env vars ([2bdaadd](https://github.com/erikarenhill/mqtt-multi-proxy/commit/2bdaaddc9d10eea1414506279d6d253be40faf0d))

## [1.2.0](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.1.3...v1.2.0) (2026-01-15)


### Features

* add keep password checkbox when editing brokers ([4ea21fb](https://github.com/erikarenhill/mqtt-multi-proxy/commit/4ea21fbad4dcc8b4ab17718e0f2faf052db975e8))
* add password encryption for broker config storage ([362af31](https://github.com/erikarenhill/mqtt-multi-proxy/commit/362af31b05c722bcab23cdc1930f4882773dc102))
* add TLS support and separate subscription topics ([f9a94ee](https://github.com/erikarenhill/mqtt-multi-proxy/commit/f9a94ee55fe975e0c9d46a99577cccd698c48d5f))
* **web-ui:** add subscription topics field for bidirectional brokers ([ebc74e3](https://github.com/erikarenhill/mqtt-multi-proxy/commit/ebc74e3be79abfecf98946ecc45589e3e8ba3cd0))


### Bug Fixes

* add graceful shutdown for broker connections ([fddb5f0](https://github.com/erikarenhill/mqtt-multi-proxy/commit/fddb5f0c5ca1f965b70345073a34f950d8f0cc22))
* always subscribe to all topics for WebUI monitoring ([2e14c92](https://github.com/erikarenhill/mqtt-multi-proxy/commit/2e14c92fa7400723f94c30481daba8d6cc84f994))
* **ci:** resolve semantic-release Date.prototype error ([8313b8b](https://github.com/erikarenhill/mqtt-multi-proxy/commit/8313b8b6e110a91d0d8efe56d560b6f36fbb38df))
* correct TypeScript setTimeout type ([deae66d](https://github.com/erikarenhill/mqtt-multi-proxy/commit/deae66d185a92907a03f07372751762f2c192fb1))
* **tests:** resolve crypto test race condition ([c4ccf28](https://github.com/erikarenhill/mqtt-multi-proxy/commit/c4ccf28563c3223f61d157bbf161b21bd51db08f))
* trigger Docker build workflow on new releases ([b1efd27](https://github.com/erikarenhill/mqtt-multi-proxy/commit/b1efd27cb91be996b366f00f875d7dee3db6ca62))


### Documentation

* add MQTT_PROXY_SECRET to docker-compose and README ([8bef8f5](https://github.com/erikarenhill/mqtt-multi-proxy/commit/8bef8f57a1c01179ee6c48051752768c4a9e25ae))

## [1.1.3](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.1.2...v1.1.3) (2026-01-08)


### Bug Fixes

* camelCase everything in config ([8436ebd](https://github.com/erikarenhill/mqtt-multi-proxy/commit/8436ebd38ff147a35b668844cc1ee7a61e2aa622))

## [1.1.2](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.1.1...v1.1.2) (2026-01-08)


### Bug Fixes

* allow only 1 bi-directional broker, subscribe anyway, we handle loopbacks in other ways ([dd594bd](https://github.com/erikarenhill/mqtt-multi-proxy/commit/dd594bde22bd0572c092a8de225280a0f735a346))

## [1.1.1](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.1.0...v1.1.1) (2026-01-08)


### Bug Fixes

* allow config.toml in docker ([b133ad5](https://github.com/erikarenhill/mqtt-multi-proxy/commit/b133ad5471851b68fbdb7941b1739da2086bb727))

## [1.1.0](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.0.2...v1.1.0) (2026-01-08)


### Features

* embedd default configs ([6acb471](https://github.com/erikarenhill/mqtt-multi-proxy/commit/6acb4714ac7dd9ff493e643f5cdee04d1848ce08))

## [1.0.2](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.0.1...v1.0.2) (2026-01-08)


### Bug Fixes

* add workflow_dispatch  to docker flow ([56602ac](https://github.com/erikarenhill/mqtt-multi-proxy/commit/56602acbd33130cccb1320106cccaa16a9cd7e94))

## [1.0.1](https://github.com/erikarenhill/mqtt-multi-proxy/compare/v1.0.0...v1.0.1) (2026-01-08)


### Bug Fixes

* pass clippy ([3f6e41b](https://github.com/erikarenhill/mqtt-multi-proxy/commit/3f6e41b06187f45212e70ad0ba674a92078d7d26))

## 1.0.0 (2026-01-08)


### Features

* add semantic release & docker builds ([62275be](https://github.com/erikarenhill/mqtt-multi-proxy/commit/62275be3307703448f6d289cfd1cb98d1e2aa19f))
* initial release ([fa284a1](https://github.com/erikarenhill/mqtt-multi-proxy/commit/fa284a18840b72d163a66bc14a5b8215c8284cba))
