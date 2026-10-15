# Test editor startup focus — 2026-10-15

Finding: the D1 runs did not establish OS keyboard focus by hiding and showing the editor. Foreground ownership alone does not establish that precondition. The existing driver's activation methods attach input queues, while winit's focus command can synthesize Alt; both are excluded by the owner's rules for this task.

Resolution: only the opt-in decoy client grants foreground rights to its freshly started editor PID with `AllowSetForegroundWindow`, uses a single explicit `SetForegroundWindow` method with the driver's bounded verification, and asks once even for an already-foreground editor window. Requests reject other processes and winit helper windows. API acceptance is recorded separately from UI Automation focus evidence. If foreground activation fails, `--operator-focus` retains the bounded manual fallback; failure to establish the precondition sends no live request.

Regression tests in `tools/s4/test_e2e_dossier.py`:

- `test_startup_grants_only_the_started_editor_pid`: grants only the recorded child PID, never all processes.
- `test_startup_foreground_request_uses_no_input_queue_attachment` and `test_startup_activation_never_uses_the_drivers_attachment_methods`: the native request and bounded verification use no input queue attachment.
- `test_startup_never_requests_another_process_or_a_helper_window`: excludes unrelated, helper and no-longer-owned windows.
- `test_startup_requests_are_opt_in_for_the_decoy`: ordinary editor runs do not request these APIs.
- `test_startup_denied_rights_do_not_replace_the_real_focus_check`: successful API acceptance cannot stand in for keyboard focus.
- `test_startup_foreground_denial_keeps_the_operator_focus_fallback`: manual arrival permits continuation, timeout sends nothing.
- `test_startup_os_denials_and_errors_are_best_effort`: denied and failed API calls return false without bypasses.

The initial three regression tests failed before implementation because the startup request methods did not exist. All desktop interactions in these tests are doubles; no window was shown or activated, no screen was captured, and no input was delivered. The Windows mechanism is best effort: the caller must have foreground rights, rights may be revoked by user input, and live effectiveness remains unverified. This change does not mark BDD-35 executed or check any Definition of Done box.
