# IP Provider Design

Building on the GitProvider approach for consistency and simplicity.

---

## Design Goals

1. **Simple default behavior** - Works without config (auto-detects primary interface)
2. **Flexible interface selection** - User can specify interface in config
3. **Multiple IP variables** - IPv4, IPv6, interface name
4. **Graceful degradation** - Empty strings if interface not found or no IP

---

## Configuration Examples

### Example 1: No Config (Auto-detect)

```toml
[prompt]
format = '{time:cyan} {hostname:yellow} {ip_address:blue} {cwd:green} $ '
```

**Behavior:**
- Auto-detect primary network interface (first non-loopback interface)
- Return IPv4 address if available
- Fall back to IPv6 if no IPv4
- Return empty string if no network connection

**Variables set:**
- `ip_address` - "192.168.1.100" (or empty)
- `ip_interface` - "eth0" (or empty)
- `ip_version` - "4" or "6" (or empty)

---

### Example 2: Specific Interface

```toml
[ip]
interface = "wlan0"  # Specify wireless interface

[prompt]
format = '{time:cyan} {hostname:yellow} {ip_address:blue} {cwd:green} $ '
```

**Behavior:**
- Use specified interface only
- Return empty if interface doesn't exist or has no IP
- Prefer IPv4, fall back to IPv6

**Variables set:**
- `ip_address` - "192.168.1.100" (from wlan0, or empty)
- `ip_interface` - "wlan0" (or empty if interface not found)
- `ip_version` - "4" or "6" (or empty)

---

### Example 3: Prefer IPv6

```toml
[ip]
interface = "eth0"
prefer_ipv6 = true  # Prefer IPv6 over IPv4

[prompt]
format = '{time:cyan} {hostname:yellow} {ip_address:blue} {cwd:green} $ '
```

**Behavior:**
- Use specified interface
- Prefer IPv6, fall back to IPv4
- Return empty if no IP

**Variables set:**
- `ip_address` - "fe80::1" (or "192.168.1.100" if no IPv6, or empty)
- `ip_interface` - "eth0"
- `ip_version` - "6" or "4" (or empty)

---

## Variable Naming Convention

Following the GitProvider pattern: `ip_` prefix for all variables

| Variable | Description | Example Value |
|----------|-------------|---------------|
| `ip_address` | The IP address (IPv4 or IPv6) | `"192.168.1.100"` or `"fe80::1"` |
| `ip_interface` | Interface name | `"eth0"`, `"wlan0"`, `"en0"` |
| `ip_version` | IP version (4 or 6) | `"4"` or `"6"` |

---

## Template Examples

### Basic IP display
```toml
format = '{ip_address:blue}'
```
Output: `192.168.1.100` (in blue)

### IP with interface name
```toml
format = '{ip_interface:dim}:{ip_address:blue}'
```
Output: `eth0:192.168.1.100`

### Conditional display (only show if IP exists)
```toml
format = '{hostname:yellow}~{ip_address:blue} {cwd:green} $ '
```
- With IP: `myhost 192.168.1.100 /home/user $`
- No IP: `myhost /home/user $` (space collapses due to conditional spacing)

### Complex example
```toml
format = '{time:cyan} {hostname:yellow}~[{ip_interface:dim}:{ip_address:blue}] {cwd:green} $ '
```
- With IP: `12:34:56 myhost [eth0:192.168.1.100] /home/user $`
- No IP: `12:34:56 myhost /home/user $`

---

## Config Schema

```toml
[ip]
interface = "eth0"           # Optional: specific interface name
                              # Default: auto-detect (first non-loopback)

prefer_ipv6 = false           # Optional: prefer IPv6 over IPv4
                              # Default: false (prefer IPv4)

# Future options (Phase 6 won't implement these):
# include_loopback = false    # Include loopback interfaces
# all_interfaces = false      # Return all IPs (for multiple [[ip]] sections)
```

---

## Implementation Approach

### Provider Structure (following GitProvider pattern)

```rust
pub struct IpProvider;

impl IpProvider {
    pub fn new() -> Self {
        Self
    }

    /// Get network interfaces
    fn get_interfaces(&self) -> Vec<Interface> {
        // Use a library like `get_if_addrs` or `nix`
    }

    /// Filter to non-loopback interfaces
    fn filter_interfaces(&self, interfaces: Vec<Interface>) -> Vec<Interface> {
        // Remove 127.0.0.1, ::1, etc.
    }

    /// Select interface based on config
    fn select_interface(
        &self,
        interfaces: Vec<Interface>,
        config_interface: Option<&str>
    ) -> Option<Interface> {
        // If config_interface specified, find it
        // Otherwise, return first non-loopback
    }

    /// Get best IP from interface
    fn get_ip_address(
        &self,
        interface: &Interface,
        prefer_ipv6: bool
    ) -> Option<(IpAddr, u8)> {
        // Return (address, version)
        // Prefer IPv4 by default, or IPv6 if prefer_ipv6=true
    }
}

impl Provider for IpProvider {
    fn name(&self) -> &str {
        "ip"
    }

    fn sections(&self) -> Vec<&str> {
        vec!["ip"]
    }

    fn collect(&self, config: &Config, validate: bool) -> ProviderResult<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // Read config
        let ip_config = config.get_section("ip");
        let interface_name = ip_config.and_then(|c| c.get("interface").and_then(|v| v.as_str()));
        let prefer_ipv6 = ip_config
            .and_then(|c| c.get("prefer_ipv6").and_then(|v| v.as_bool()))
            .unwrap_or(false);

        // Get interfaces
        let interfaces = self.get_interfaces();
        let filtered = self.filter_interfaces(interfaces);

        // Select interface
        let selected = self.select_interface(filtered, interface_name);

        if let Some(iface) = selected {
            vars.insert("ip_interface".to_string(), iface.name.clone());

            if let Some((addr, version)) = self.get_ip_address(&iface, prefer_ipv6) {
                vars.insert("ip_address".to_string(), addr.to_string());
                vars.insert("ip_version".to_string(), version.to_string());
            }
        }

        Ok(vars)
    }

    fn default_config(&self) -> HashMap<String, Value> {
        let mut defaults = HashMap::new();
        defaults.insert("ip".to_string(), json!({
            "prefer_ipv6": false
        }));
        defaults
    }

    fn cacheable(&self) -> bool {
        // IP addresses can change, but slowly
        // Could be cacheable with longer TTL
        true
    }

    fn cache_duration(&self) -> u64 {
        // Cache for 30 seconds (IPs don't change that often)
        30
    }
}
```

---

## Libraries to Use

**Option 1: `get_if_addrs` crate** (recommended)
- Simple, cross-platform
- Returns list of interfaces with IPs
- Handles IPv4 and IPv6

```toml
[dependencies]
get_if_addrs = "0.5"
```

**Option 2: `nix` crate**
- More low-level
- Linux/Unix only
- More control but more complex

---

## Error Handling

Following GitProvider pattern:

| Scenario | Behavior |
|----------|----------|
| No network interfaces | Return empty vars |
| Specified interface not found | Return empty vars |
| Interface has no IP | Return empty vars (but set `ip_interface`) |
| Permission denied | Return empty vars |
| `validate=true` | Return `ProviderError::ResourceNotAvailable` |

---

## Testing Strategy

Similar to GitProvider:

1. **Unit tests** for interface selection logic
2. **Integration tests** (may need to mock interfaces)
3. **Manual testing** on different network configurations

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_filter_loopback() {
        // Test that loopback interfaces are filtered out
    }

    #[test]
    fn test_select_specific_interface() {
        // Test that config interface name is respected
    }

    #[test]
    fn test_prefer_ipv6() {
        // Test IPv6 preference logic
    }
}
```

---

## Example Outputs

### Scenario 1: Home network (single interface)
**Config:** Auto-detect
**Output:** `192.168.1.100`
**Variables:**
```
ip_address = "192.168.1.100"
ip_interface = "wlan0"
ip_version = "4"
```

### Scenario 2: Dual-stack network (IPv4 + IPv6)
**Config:** `prefer_ipv6 = true`
**Output:** `2001:db8::1`
**Variables:**
```
ip_address = "2001:db8::1"
ip_interface = "eth0"
ip_version = "6"
```

### Scenario 3: No network
**Config:** Auto-detect
**Output:** (empty)
**Variables:**
```
(all empty)
```

### Scenario 4: Specific interface not found
**Config:** `interface = "eth99"`
**Output:** (empty)
**Variables:**
```
(all empty)
```

---

## Future Enhancements (Beyond Phase 6)

1. **Multiple interfaces** - Use `[[ip]]` array syntax
   ```toml
   [[ip]]
   name = "wan"
   interface = "eth0"

   [[ip]]
   name = "lan"
   interface = "eth1"
   ```
   Creates: `{wan}`, `{lan}` variables

2. **All IPs** - Show all IPs from all interfaces
   ```toml
   [ip]
   all_interfaces = true
   format = "{interface}:{address}"
   ```

3. **Link-local filtering** - Exclude link-local addresses (169.254.x.x, fe80::)

4. **Public IP detection** - Query external service for public IP

---

## Questions to Resolve

1. **Default interface selection** - Should we prefer ethernet over wireless?
2. **IPv6 link-local** - Include or exclude fe80:: addresses?
3. **Multiple IPs per interface** - Show first one only, or all?
4. **Interface name format** - Platform differences (eth0 vs en0 vs enp3s0)?

---

## Summary

This design:
- ✅ Follows GitProvider pattern for consistency
- ✅ Simple default (auto-detect)
- ✅ Flexible config (interface selection)
- ✅ Multiple variables (address, interface, version)
- ✅ Graceful degradation (empty on error)
- ✅ Cross-platform support (via get_if_addrs)
- ✅ Cacheable (30s TTL)
- ✅ Testable (unit tests for logic)

Ready to implement once design is approved!
