# Changelog for `stuff`

## 0.2.0

### **Breaking changes**
* Renamed all occurrences of `extra` to `other` (for example in methods)

### Improvements
* Made `(): StuffingStrategy` generic over all backends (with some trait bounds) instead of just `usize`, `u64`, and `u128`.
* Added an MSRV (minimum supported rust version) policy (MSRV of `1.31.0`)
* Upgraded `sptr` dependency to `0.3.1`