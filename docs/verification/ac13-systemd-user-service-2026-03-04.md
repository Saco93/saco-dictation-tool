# AC13 Manual Verification Evidence (systemd user service)

Date: 2026-03-04  
Verifier: Saco (captured by automated BMAD execution loop)

## Objective

Validate AC13 operational evidence:
- `sttd.service` is installable/enabled in user systemd scope.
- Service restarts cleanly.
- `sttctl status` works during normal startup window.

## Commands and Observed Output

```text
## command: date -u
Wed Mar  4 14:22:00 UTC 2026
(exit=0)

## command: systemd-analyze verify config/sttd.service
(exit=0)

## command: systemctl --user is-enabled sttd.service
enabled
(exit=0)

## command: systemctl --user is-active sttd.service
active
(exit=0)

## command: systemctl --user restart sttd.service
(exit=0)

## command: systemctl --user is-active sttd.service
active
(exit=0)

## command: for i in 1 2 3 4 5; do target/release/sttctl status && break; sleep 0.2; done
Error: failed to connect to daemon at /run/user/1000/sttd/sttd.sock

Caused by:
    ipc transport failed: Connection refused (os error 111)
state=Idle protocol_version=1 cooldown_remaining_seconds=0 requests_in_last_minute=0
(exit=0)

## command: systemctl --user status sttd.service --no-pager --full | sed -n "1,20p"
● sttd.service - sttd daemon (Hyprland-native speech-to-text)
     Loaded: loaded (/home/saco/.config/systemd/user/sttd.service; enabled; preset: enabled)
     Active: active (running) since Wed 2026-03-04 22:22:01 CST; 237ms ago
 Invocation: 90d1554a5a954fc1a5f694bc4957e1f2
   Main PID: 3772101 (sttd)
      Tasks: 17 (limit: 37709)
     Memory: 3M (peak: 4.3M)
        CPU: 25ms
     CGroup: /user.slice/user-1000.slice/user@1000.service/app.slice/sttd.service
             └─3772101 /home/saco/Projects/Rust/saco-dictation-tool/master/target/release/sttd --config /home/saco/.config/sttd/sttd.toml

Mar 04 22:22:01 saco-zenbook-archlinux systemd[934]: Started sttd daemon (Hyprland-native speech-to-text).
Mar 04 22:22:01 saco-zenbook-archlinux sttd[3772101]: 2026-03-04T14:22:01.095021Z  INFO sttd: audio capture device initialized device=default sample_rate_hz=44100 channels=2
Mar 04 22:22:01 saco-zenbook-archlinux sttd[3772101]: 2026-03-04T14:22:01.095074Z  INFO sttd: sttd daemon starting socket=/run/user/1000/sttd/sttd.sock
(exit=0)
```

## Result

- `config/sttd.service` passes `systemd-analyze verify`.
- User service is enabled and active.
- After restart, client sees a brief socket warm-up window (one connection-refused), then `sttctl status` succeeds.
- AC13 operational startup evidence is now explicitly recorded.
