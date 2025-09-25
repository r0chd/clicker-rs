# wl-clicker-rs

Autoclicker for wayland with blackjack and hookers

## Features

- Multiple profiles
- Global hotkeys
- Clicks use Gaussian + Poisson timing with added mouse jitter for realistic, undetectable behavior

## Config Example

configs can be found at `/etc/wl-clicker/default.nix` or `/etc/wl-clicker.nix`

```nix
{
  profiles = [
    {
      name = "default";               # Profile name
      activation_keys = [ "KEY_F8" ]; # Keys that must be held to activate this profile
      cps = {
        target = 15.0;                # Target clicks per second
        std_dev = 1.5;                # Standard deviation of CPS (optional, defaults to 1.5)
      };
      jitter = 1.0;                   # Slight random mouse movement during clicks to mimic human behavior
      toggle = true;                  # true: press activation keys once to toggle profile
                                      # false: profile is active only while activation keys are held
      repeat_key = "BTN_LEFT";        # Mouse button to click (optional, defaults to BTN_LEFT)
    }
    {
      name = "right_click";
      activation_keys = [ "KEY_F8" ];
      cps = {
        target = 15.0;
        std_dev = 1.5;
      };
      jitter = 1.0;                   # Slight random mouse movement during clicks
      toggle = false;
      repeat_key = "BTN_RIGHT";       # Use right mouse button for this profile (defaults to BTN_LEFT)
    }
  ];
}
```
