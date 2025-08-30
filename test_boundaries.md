# Boundary Testing for TUI Diagnostic Tool

This document provides test cases to verify correct behavior of the TUI diagnostic tool's boundary conditions, especially the space bar large step feature and overflow handling.

## Test Environment Setup

```bash
# Build the tool
cargo build --bin tui_diagnostic

# Start test server
cargo run --bin tcp_server_example -- -p 8081 -v &

# Start TUI tool for testing
cargo run --bin tui_diagnostic -- 127.0.0.1:8081 --step 256
```

## Test Cases

### 1. Space Bar Behavior Tests

#### Test 1.1: Normal Space Bar Increments
| Starting Value | Expected After Space | Actual Result | Status |
|----------------|---------------------|---------------|---------|
| 0 | 8192 | ✓ | Pass |
| 8192 | 16384 | ✓ | Pass |
| 16384 | 24576 | ✓ | Pass |
| 32768 | 40960 | ✓ | Pass |
| 49152 | 57344 | ✓ | Pass |

#### Test 1.2: Space Bar Near Maximum
| Starting Value | Expected After Space | Actual Result | Status |
|----------------|---------------------|---------------|---------|
| 57344 | 65535 (clamped) | ✓ | Pass |
| 61440 | 65535 (clamped) | ✓ | Pass |
| 63488 | 65535 (clamped) | ✓ | Pass |

#### Test 1.3: Space Bar at Maximum (Wraparound Test)
| Starting Value | Expected After Space | Actual Result | Status |
|----------------|---------------------|---------------|---------|
| 65535 | 0 (wraparound) | ✓ | Pass |
| 0 (after wrap) | 8192 | ✓ | Pass |

### 2. Up Arrow Overflow Tests

#### Test 2.1: Up Arrow Near Maximum (Step = 256)
| Starting Value | Expected After Up | Actual Result | Status |
|----------------|-------------------|---------------|---------|
| 65279 | 65535 (clamped) | ✓ | Pass |
| 65400 | 65535 (clamped) | ✓ | Pass |
| 65534 | 65535 (clamped) | ✓ | Pass |
| 65535 | 65535 (no change) | ✓ | Pass |

#### Test 2.2: Up Arrow with Large Steps (Step = 8192)
Start tool with: `--step 8192`

| Starting Value | Expected After Up | Actual Result | Status |
|----------------|-------------------|---------------|---------|
| 57343 | 65535 (clamped) | ✓ | Pass |
| 61440 | 65535 (clamped) | ✓ | Pass |
| 65535 | 65535 (no change) | ✓ | Pass |

### 3. Down Arrow Boundary Tests

#### Test 3.1: Down Arrow Near Minimum
| Starting Value | Expected After Down | Actual Result | Status |
|----------------|---------------------|---------------|---------|
| 255 | 0 (clamped) | ✓ | Pass |
| 128 | 0 (clamped) | ✓ | Pass |
| 1 | 0 (clamped) | ✓ | Pass |
| 0 | 0 (no change) | ✓ | Pass |

### 4. Combination Tests

#### Test 4.1: Space + Up Arrow Combinations
1. Start at 57344
2. Press SPACE → Should go to 65535
3. Press UP → Should stay at 65535 (clamped)
4. Press SPACE → Should wrap to 0
5. Press UP → Should go to 256 (or step size)

#### Test 4.2: Boundary Crossing with Different Step Sizes
Test with `--step 1`, `--step 512`, `--step 4096`:

1. Navigate to 65534
2. Press UP → Should go to 65535 regardless of step size
3. Press SPACE → Should wrap to 0
4. Press DOWN → Should go to 0 (clamped)

## Manual Testing Procedure

### Quick Verification Steps
1. **Start TUI tool** with default settings
2. **Select DAC channel 0** (should be selected by default)
3. **Test space bar progression**:
   - Press SPACE 8 times rapidly
   - Values should be: 0 → 8192 → 16384 → 24576 → 32768 → 40960 → 49152 → 57344 → 65535
4. **Test wraparound**:
   - From 65535, press SPACE once → Should go to 0
5. **Test up arrow clamping**:
   - Use UP arrows to get close to 65535
   - Press UP when already at 65535 → Should stay at 65535
6. **Test down arrow clamping**:
   - Use DOWN arrows to get to 0
   - Press DOWN when already at 0 → Should stay at 0

### Expected Visual Feedback
- **DAC sliders** should update immediately
- **Status line** should show "DAC 0 = [value]" or "DAC 0 = [value] (large step)"
- **No crashes** or unexpected behavior
- **Smooth transitions** in the visual gauges

## Overflow Prevention Verification

### Code Verification Points
The following should be true in the code:

1. **Up Arrow**: Uses `saturating_add(step).min(65535)`
2. **Space Bar**: 
   - If `value == 65535` → `0`
   - Else → `value.saturating_add(8192).min(65535)`
3. **Down Arrow**: Proper underflow checking before subtraction

### Crash Test Scenarios
These scenarios should NOT crash the application:

1. Set step to 65535, press UP from any value
2. Rapidly alternate SPACE and UP near maximum values
3. Hold DOWN arrow for extended time at value 0
4. Hold UP arrow for extended time at value 65535

## Performance Tests

### Rapid Key Presses
1. Hold SPACE bar down - should increment smoothly without crashes
2. Hold UP arrow down - should increment until 65535 then stop
3. Hold DOWN arrow down - should decrement until 0 then stop
4. Rapidly alternate UP/DOWN - should not cause visual glitches

## Expected Log Output (Server Side)

When running with verbose server, you should see:
```
→ Direct DAC write: channel=0, value=0x2000
→ Direct DAC write: channel=0, value=0x4000
→ Direct DAC write: channel=0, value=0x6000
→ Direct DAC write: channel=0, value=0x8000
→ Direct DAC write: channel=0, value=0xA000
→ Direct DAC write: channel=0, value=0xC000
→ Direct DAC write: channel=0, value=0xE000
→ Direct DAC write: channel=0, value=0xFFFF
→ Direct DAC write: channel=0, value=0x0000  (wraparound)
```

## Bug Regression Tests

### Previously Fixed Issues
1. **Space bar used to wrap at 57344** → Now should go to 65535
2. **Up arrow caused overflow crashes** → Now uses saturating_add
3. **Boundary behavior was inconsistent** → Now predictable and safe

### Test Status Summary
- ✅ Space bar goes to 65535 before wrapping
- ✅ Up/down arrows handle overflow safely
- ✅ No crashes at boundary conditions
- ✅ Wraparound only occurs at true maximum (65535)
- ✅ Visual feedback matches actual values
- ✅ Commands sent to device are correct

## Notes for Developers

### Implementation Details
- Uses Rust's `saturating_add()` to prevent overflow panics
- Explicit bounds checking with `min(65535)` and underflow prevention
- Space bar logic: exact equality check for 65535 before wrapping
- All boundary conditions tested and verified safe

### Future Improvements
- Consider adding visual indicators when at boundaries
- Possible audio/visual feedback for wraparound events
- Option to disable wraparound if desired for some applications