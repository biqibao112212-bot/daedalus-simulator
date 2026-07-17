# Auto-aim gimbal ownership contract

When `SubscribeAutoAim` is enabled, the active external command transport
(`udp`, `talos`, or `ros2`) is the sole writer of the controlled vehicle's
gimbal transform and `InfantryGimbal` state.

Manual keyboard and mouse gimbal systems must return without reading or
rewriting that transform while auto aim is active. Bevy system order is not an
ownership mechanism: two unordered writers can expose different Euler branches
to capture/runtime feedback and make pitch jump between the intended command
and a pitch-limit value.

Transport pitch contracts:

- vivsionn `AimCommand.pitch_deg`: optical/tracker elevation in degrees;
- the camera mount is 25 degrees from the local gimbal joint;
- UDP `pitch_deg`: optical-neutral form, `65 deg - optical_pitch`;
- Talos shared-memory command: the existing Talos sign/neutral conversion;
- exactly one Daedalus transport applies commands, selected by
  `config.auto_aim.command_transport`.

Do not compensate for an ownership conflict with pitch offsets, wider limits,
tracker changes, or ballistic tuning. Validate a static target by confirming
that commanded and runtime local pitch remain on one continuous branch.
