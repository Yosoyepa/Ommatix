//! The freshness check (SDD §2.9, chapter 04 §4.2): right before a live delivery the whole
//! screen is captured again and compared exactly with the committed base. That path has no
//! tolerance and no caret filter, so a single pixel that changed after the observation refuses
//! the action with `STALE_SNAPSHOT`, wherever it is on the screen, and nothing is sent. These
//! tests change one pixel of the form that [`super`] paints.

use super::*;

/// Far from every element of `form()`: where a blinking caret would not be.
const FAR_PIXEL: (u32, u32) = (630, 10);

/// Inside the Save button and outside its caption: a hover or focus mark on the target.
const TARGET_PIXEL: (u32, u32) = (405, 255);

/// Observes `form()`, paints one pixel of the screen white, and sends a live click on Save
/// that expects no effect.
fn live_click_after_one_pixel(pixel: (u32, u32)) -> (Rig, ObservePage, ActionResult) {
    let mut r = rig(form());
    r.engine.config.action_freshness = ActionFreshness::Screen;
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world().pixels.push((pixel.0, pixel.1, [255, 255, 255]));
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r1", ExpectedEffect::None));
    (r, page, result)
}

/// `STALE_SNAPSHOT` as chapter 07 §7.7 gives it (not retryable, observe again), and no event
/// reached the actuator: no delivery, no focus click, no keys.
fn assert_stale_and_nothing_sent(r: &Rig, result: &ActionResult) {
    assert_eq!(result.status, ActionStatus::Rejected, "{result:?}");
    assert!(!result.input_delivered(), "{result:?}");
    let error = result.error.as_ref().expect("an error");
    assert_eq!(
        (error.code, error.retryable, error.next_step),
        (ErrorCode::StaleSnapshot, false, NextStep::ObserveAgain)
    );
    assert!(
        error
            .message
            .contains("the screen changed since the observation"),
        "{}",
        error.message
    );
    let w = r.world();
    assert!(
        w.delivered.is_empty() && w.clicks.is_empty() && w.keys.is_empty(),
        "the actuator was handed nothing"
    );
    assert!(
        !w.events
            .iter()
            .any(|e| matches!(*e, "deliver" | "click" | "keys")),
        "{:?}",
        w.events
    );
}

/// A single pixel changed far from the target after the observation: the live click is refused
/// with `STALE_SNAPSHOT` and nothing is sent. The refused click is not retried on the spent
/// snapshot, even once the pixel is back; a new observation makes the click deliverable.
#[test]
fn a_single_pixel_change_before_delivery_is_stale_and_sends_nothing() {
    let (mut r, page, result) = live_click_after_one_pixel(FAR_PIXEL);
    assert_stale_and_nothing_sent(&r, &result);

    // The pixel goes back, so the screen matches the base again, but the snapshot is spent.
    r.world().pixels.clear();
    let retry = r
        .engine
        .execute_action(&live_click(&page, "r2", ExpectedEffect::None));
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    assert!(r.world().delivered.is_empty(), "no click");

    // A new observation is the way on.
    let again = r.observe();
    let result = r
        .engine
        .execute_action(&live_click(&again, "r3", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    let m = r.engine.engine_metrics();
    assert_eq!((m.delivered_actions, m.rejected_actions), (1, 2));
}

/// The same holds when the changed pixel belongs to the target itself: nothing is sent.
#[test]
fn a_single_pixel_change_under_the_target_is_stale_and_sends_nothing() {
    let (r, _page, result) = live_click_after_one_pixel(TARGET_PIXEL);
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn a_dry_run_never_captures_or_predicts_pixel_freshness() {
    let mut r = rig(form());
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.world()
        .pixels
        .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
    r.world().fail_capture = true;
    r.world().events.clear();
    let mut request = live_click(&page, "dry", ExpectedEffect::None);
    request.dry_run = true;
    let result = r.engine.execute_action(&request);
    assert_eq!(result.status, ActionStatus::Simulated);
    assert!(!r.world().events.contains(&"capture"));
    assert!(r.world().delivered.is_empty(), "no synthetic delivery");
    assert_eq!(
        r.engine.last_action_timings().expect("timings").freshness,
        Duration::ZERO
    );
}

fn scoped_window(handle: u64, z_order: u32, rectangle: BBox) -> TopLevelWindow {
    TopLevelWindow {
        class: "TestWindow".into(),
        handle,
        rectangle: Some(rectangle),
        z_order,
    }
}

fn scoped_rig() -> Rig {
    let mut r = rig(form());
    r.engine.config.action_freshness = ActionFreshness::TargetWindow;
    r.world().live = true;
    r.set_policy(live_policy());
    let window = scoped_window(1, 0, bx(64.0, 64.0, 576.0, 320.0));
    r.world().foreground_window = Some(window.clone());
    r.world().window_at_point = Some(window.clone());
    r.world().top_windows = Some(vec![window]);
    r
}

#[test]
fn ut_55_a_change_outside_the_target_group_is_accepted_in_target_window_mode() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world()
        .pixels
        .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    assert_eq!(r.world().delivered.len(), 1);
    let (_, _, screen) = live_click_after_one_pixel(FAR_PIXEL);
    assert_eq!(error_code(&screen), Some(ErrorCode::StaleSnapshot));
}

#[test]
fn ut_56_a_one_pixel_change_inside_the_target_window_refuses_far_from_the_target() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().pixels.push((100, 100, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
    let diagnostic = r
        .engine
        .last_action_timings()
        .unwrap()
        .freshness_diff
        .unwrap();
    assert_eq!(
        (
            diagnostic.total_tiles,
            diagnostic.group_tiles,
            diagnostic.touches_target
        ),
        (1, Some(1), false)
    );
}

#[test]
fn ut_57_a_window_overlapping_the_target_window_is_in_the_group() {
    let mut r = scoped_rig();
    r.world()
        .top_windows
        .as_mut()
        .unwrap()
        .push(scoped_window(2, 1, bx(500.0, 0.0, 640.0, 100.0)));
    let page = r.observe();
    r.world().pixels.push((630, 10, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
    assert_eq!(
        r.engine
            .last_action_timings()
            .unwrap()
            .freshness_diff
            .unwrap()
            .group_tiles,
        Some(1)
    );
}

#[test]
fn ut_58_a_moved_target_window_refuses_even_when_the_old_rectangle_is_unchanged() {
    let mut r = scoped_rig();
    let page = r.observe();
    let moved = scoped_window(1, 0, bx(65.0, 64.0, 576.0, 320.0));
    r.world().foreground_window = Some(moved.clone());
    r.world().window_at_point = Some(moved.clone());
    r.world().top_windows = Some(vec![moved]);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn ut_59_a_change_of_group_order_or_foreground_refuses() {
    for foreground_changes in [false, true] {
        let mut r = scoped_rig();
        let second = scoped_window(2, 1, bx(500.0, 0.0, 640.0, 100.0));
        r.world().top_windows.as_mut().unwrap().push(second.clone());
        let page = r.observe();
        if foreground_changes {
            r.world().foreground_window = Some(second);
        } else {
            r.world().top_windows.as_mut().unwrap()[0].z_order = 2;
        }
        let result = r
            .engine
            .execute_action(&live_click(&page, "r", ExpectedEffect::None));
        assert_stale_and_nothing_sent(&r, &result);
    }
}

#[test]
fn ut_60_a_geometry_or_epoch_change_refuses_in_target_window_mode() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().epoch += 1;
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn ut_61_an_actuator_that_cannot_attest_rectangles_checks_the_whole_frame() {
    for fallback in 0..3 {
        let mut r = scoped_rig();
        if fallback == 0 {
            r.world().top_windows = None;
        }
        if fallback == 1 {
            r.world().top_windows.as_mut().unwrap()[0].rectangle = None;
        }
        if fallback == 2 {
            let small = scoped_window(1, 0, bx(400.0, 250.0, 410.0, 260.0));
            r.world().top_windows = Some(vec![small.clone()]);
            r.world().foreground_window = Some(small.clone());
            r.world().window_at_point = Some(small);
        }
        let page = r.observe();
        r.world()
            .pixels
            .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
        let result = r
            .engine
            .execute_action(&live_click(&page, "r", ExpectedEffect::None));
        assert_stale_and_nothing_sent(&r, &result);
        assert_eq!(
            r.engine
                .last_action_timings()
                .unwrap()
                .freshness_diff
                .unwrap()
                .group_tiles,
            None
        );
    }
}

#[test]
fn ut_62_the_visual_effect_is_measured_against_the_whole_frame() {
    let mut results = Vec::new();
    for scope in [ActionFreshness::Screen, ActionFreshness::TargetWindow] {
        let mut r = scoped_rig();
        r.engine.config.action_freshness = scope;
        r.world().react_to_input = true;
        let page = r.observe();
        results.push(
            r.engine
                .execute_action(&live_click(&page, "r", ExpectedEffect::VisualChange))
                .status,
        );
    }
    assert_eq!(results, vec![ActionStatus::EffectObserved; 2]);
}

/// Differential scalar expectation over every tile, with boundary and maximum margins.
#[test]
fn ut_63_target_window_accepts_exactly_when_no_changed_tile_meets_the_group() {
    for split in [false, true] {
        for margin in [0, 1, 64, u32::MAX] {
            for row in 0..5 {
                for col in 0..10 {
                    let x = col * 64 + (col * 17 + row * 7) % 64;
                    let y = row * 64 + (col * 11 + row * 3) % 64;
                    let mut r = scoped_rig();
                    let rectangles = if split {
                        let target = scoped_window(1, 1, bx(384.0, 192.0, 576.0, 320.0));
                        let foreground = scoped_window(2, 0, bx(0.0, 0.0, 64.0, 64.0));
                        r.world().top_windows = Some(vec![target.clone(), foreground.clone()]);
                        r.world().window_at_point = Some(target);
                        r.world().foreground_window = Some(foreground);
                        vec![(384.0, 192.0, 576.0, 320.0), (0.0, 0.0, 64.0, 64.0)]
                    } else {
                        vec![(64.0, 64.0, 576.0, 320.0)]
                    };
                    r.engine.config.action_freshness_margin_px = margin;
                    let page = r.observe();
                    r.world().pixels.push((x, y, [255, 255, 255]));
                    let left = f64::from(col * 64);
                    let top = f64::from(row * 64);
                    let m = f64::from(margin);
                    // Independent scalar interval intersection, not the production helper.
                    let intersects = rectangles.iter().any(|&(x0, y0, x1, y1)| {
                        left < x1 + m && left + 64.0 > x0 - m && top < y1 + m && top + 64.0 > y0 - m
                    });
                    let result = r.engine.execute_action(&live_click(
                        &page,
                        "generated",
                        ExpectedEffect::None,
                    ));
                    assert_eq!(
                        error_code(&result),
                        intersects.then_some(ErrorCode::StaleSnapshot),
                        "pixel {x},{y}; margin {margin}; split {split}"
                    );
                    assert_eq!(r.world().delivered.len(), usize::from(!intersects));
                    let (_, _, screen) = live_click_after_one_pixel((x, y));
                    assert_eq!(error_code(&screen), Some(ErrorCode::StaleSnapshot));
                }
            }
        }
    }
}

#[test]
fn changing_or_losing_group_attestation_around_the_action_capture_checks_the_whole_frame() {
    for changed in [false, true] {
        let mut r = scoped_rig();
        let page = r.observe();
        let first = r.world().top_windows.clone();
        let mut second = first.clone();
        if changed {
            second.as_mut().unwrap()[0].z_order = 1;
        } else {
            second = None;
        }
        r.world().top_windows_script.extend([first, second]);
        r.world()
            .pixels
            .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
        let result = r
            .engine
            .execute_action(&live_click(&page, "r", ExpectedEffect::None));
        assert_stale_and_nothing_sent(&r, &result);
        assert_eq!(
            r.engine
                .last_action_timings()
                .unwrap()
                .freshness_diff
                .unwrap()
                .group_tiles,
            None
        );
    }
}

#[test]
fn losing_group_attestation_after_the_last_look_cannot_ignore_changed_pixels() {
    let mut r = scoped_rig();
    let page = r.observe();
    let attested = r.world().top_windows.clone();
    r.world()
        .top_windows_script
        .extend([attested.clone(), attested, None]);
    r.world()
        .pixels
        .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn a_group_change_after_the_last_look_spends_the_snapshot_without_a_pixel_change() {
    let mut r = scoped_rig();
    let page = r.observe();
    let original = r.world().top_windows.clone();
    let mut overlay = original.clone();
    overlay
        .as_mut()
        .unwrap()
        .push(scoped_window(3, 1, bx(500.0, 0.0, 640.0, 100.0)));
    r.world()
        .top_windows_script
        .extend([original.clone(), original, overlay]);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
    let d = r
        .engine
        .last_action_timings()
        .unwrap()
        .freshness_diff
        .unwrap();
    assert_eq!((d.total_tiles, d.group_tiles), (0, Some(0)));
    let retry = r
        .engine
        .execute_action(&live_click(&page, "retry", ExpectedEffect::None));
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
}

#[test]
fn a_policy_change_keeps_its_refusal_before_group_identity_is_checked() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap()[0].z_order = 2;
    r.world()
        .foreground_script
        .extend([Some(APP.into()), Some("other-app".into())]);
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_eq!(error_code(&result), Some(ErrorCode::PolicyDenied));
    assert!(r.world().delivered.is_empty(), "nothing sent");
    // Policy keeps its refusal code, but the already-known mismatch still spends the
    // observation. Restoring both the policy and group cannot revive that snapshot.
    r.world().top_windows.as_mut().unwrap()[0].z_order = 0;
    r.world().foreground_script.clear();
    let retry = r.engine.execute_action(&live_click(
        &page,
        "spent-after-policy",
        ExpectedEffect::None,
    ));
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    assert!(
        r.world().delivered.is_empty(),
        "known mismatch remains spent after policy refusal"
    );
}

#[test]
fn a_dry_run_in_target_window_mode_never_reads_windows_or_captures_again() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().events.clear();
    r.world().fail_capture = true;
    let mut request = live_click(&page, "dry", ExpectedEffect::None);
    request.dry_run = true;
    assert_eq!(
        r.engine.execute_action(&request).status,
        ActionStatus::Simulated
    );
    assert!(
        !r.world()
            .events
            .iter()
            .any(|e| matches!(*e, "windows" | "capture" | "window_at"))
    );
}

#[test]
fn type_checks_target_window_freshness_only_before_its_focusing_click() {
    let mut r = typing::typing_rig();
    r.engine.config.action_freshness = ActionFreshness::TargetWindow;
    let window = scoped_window(1, 0, bx(64.0, 0.0, 576.0, 320.0));
    r.world().top_windows = Some(vec![window.clone()]);
    r.world().foreground_window = Some(window.clone());
    r.world().window_at_point = Some(window);
    r.world().focused = Some(typing::name_field());
    r.world().react_to_click = true;
    let page = r.observe();
    r.world().events.clear();
    let result = r
        .engine
        .execute_action(&typing::typing(&page, "type", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    let world = r.world();
    assert_eq!((world.clicks.len(), world.keys.len()), (1, 1));
    let click = world.events.iter().position(|e| *e == "click").unwrap();
    assert!(!world.events[click + 1..].contains(&"windows"));
}

#[test]
fn the_operators_stop_during_the_final_group_read_sends_nothing() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().events.clear();
    r.stop_at("windows", 3);
    let result = r
        .engine
        .execute_action(&live_click(&page, "stop", ExpectedEffect::None));
    assert_eq!(error_code(&result), Some(ErrorCode::PolicyDenied));
    assert!(
        r.world().delivered.is_empty(),
        "the stop owns this boundary"
    );
}

#[test]
fn an_unstable_window_read_during_observation_keeps_whole_frame_freshness() {
    let mut r = scoped_rig();
    let first = r.world().top_windows.clone();
    let mut moved = first.clone();
    moved.as_mut().unwrap()[0].z_order = 1;
    r.world().top_windows_script.extend([first, moved]);
    let page = r.observe();
    r.world()
        .pixels
        .push((FAR_PIXEL.0, FAR_PIXEL.1, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn the_window_group_is_one_hop_from_the_two_seeds_not_a_transitive_closure() {
    let mut r = scoped_rig();
    let seed = scoped_window(1, 0, bx(384.0, 192.0, 576.0, 320.0));
    r.world().foreground_window = Some(seed.clone());
    r.world().window_at_point = Some(seed.clone());
    r.world().top_windows = Some(vec![
        seed,
        scoped_window(2, 1, bx(256.0, 64.0, 448.0, 256.0)),
        scoped_window(3, 2, bx(128.0, 0.0, 320.0, 128.0)),
    ]);
    let page = r.observe();
    r.world().pixels.push((130, 10, [255, 255, 255]));
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
}

#[test]
fn window_read_order_is_normalized_by_z_order_then_handle() {
    let mut r = scoped_rig();
    r.world()
        .top_windows
        .as_mut()
        .unwrap()
        .push(scoped_window(2, 0, bx(500.0, 0.0, 640.0, 100.0)));
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap().reverse();
    let result = r
        .engine
        .execute_action(&live_click(&page, "r", ExpectedEffect::None));
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
}

#[test]
fn a_resized_window_that_no_longer_contains_the_target_still_refuses_by_identity() {
    let mut r = scoped_rig();
    let page = r.observe();
    let resized = scoped_window(1, 0, bx(450.0, 192.0, 576.0, 320.0));
    r.world().top_windows = Some(vec![resized.clone()]);
    r.world().foreground_window = Some(resized.clone());
    r.world().window_at_point = Some(resized);
    let result = r
        .engine
        .execute_action(&live_click(&page, "resize", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn a_known_resize_cannot_hide_behind_an_unavailable_point_attestation() {
    let mut r = scoped_rig();
    let page = r.observe();
    let resized = scoped_window(1, 0, bx(450.0, 192.0, 576.0, 320.0));
    r.world().top_windows = Some(vec![resized.clone()]);
    r.world().foreground_window = Some(resized);
    r.world().window_at_point = None;
    let result = r
        .engine
        .execute_action(&live_click(&page, "resize", ExpectedEffect::None));
    assert_stale_and_nothing_sent(&r, &result);
}

/// A successful later read or missing evidence cannot erase a mismatch already attested.
#[test]
fn a_known_group_change_refuses_even_when_the_final_read_is_unavailable() {
    for sequence in [
        [1, 1, 2],
        [1, 2, 2],
        [0, 1, 2],
        [1, 0, 2],
        [1, 1, 0],
        [1, 0, 0],
        [0, 1, 0],
    ] {
        let mut r = scoped_rig();
        let page = r.observe();
        let observed = r.world().top_windows.clone();
        let mut changed = observed.clone();
        changed.as_mut().unwrap()[0].z_order += 1;
        r.world()
            .top_windows_script
            .extend(sequence.map(|read| match read {
                0 => observed.clone(),
                1 => changed.clone(),
                _ => None,
            }));
        r.world().events.clear();
        let result =
            r.engine
                .execute_action(&live_click(&page, "known-change", ExpectedEffect::None));
        assert_stale_and_nothing_sent(&r, &result);
        let diagnostic = r
            .engine
            .last_action_timings()
            .unwrap()
            .freshness_diff
            .unwrap();
        assert_eq!(
            diagnostic.total_tiles, 0,
            "identical frame bytes for {sequence:?}"
        );
        r.world().top_windows_script.clear();
        let retry =
            r.engine
                .execute_action(&live_click(&page, "spent-change", ExpectedEffect::None));
        assert_eq!(
            error_code(&retry),
            Some(ErrorCode::StaleSnapshot),
            "snapshot spent for {sequence:?}"
        );
        assert!(
            r.world().delivered.is_empty(),
            "no delivery after {sequence:?}"
        );
    }
}

/// The foreground query in the existing last look crosses the age bound after its
/// age check. Screen must retain that order; only target_window adds another check.
fn click_expiring_during_the_last_foreground_read(scope: ActionFreshness) -> (Rig, ActionResult) {
    struct ForegroundAgingClock {
        world: Shared,
        base: Arc<ManualClock>,
    }
    impl Clock for ForegroundAgingClock {
        fn now(&self) -> Duration {
            let late = lock(&self.world)
                .events
                .iter()
                .filter(|e| **e == "foreground")
                .count()
                >= 2;
            self.base.now()
                + if late {
                    Duration::from_millis(2)
                } else {
                    Duration::ZERO
                }
        }
        fn sleep(&self, duration: Duration) {
            self.base.sleep(duration);
        }
    }
    let mut r = rig(form());
    r.engine.config.action_freshness = scope;
    r.engine.config.max_snapshot_age = Duration::from_millis(100);
    r.world().live = true;
    r.set_policy(live_policy());
    let page = r.observe();
    r.clock.advance(Duration::from_millis(99));
    r.world().events.clear();
    r.engine.ports.clock = Arc::new(ForegroundAgingClock {
        world: r.world.clone(),
        base: r.clock.clone(),
    });
    let result =
        r.engine
            .execute_action(&live_click(&page, "late-foreground", ExpectedEffect::None));
    (r, result)
}

#[test]
fn screen_preserves_the_original_age_check_sequence_through_the_last_look() {
    let (r, result) = click_expiring_during_the_last_foreground_read(ActionFreshness::Screen);
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
    assert_eq!(
        r.world().events,
        ["foreground", "capture", "foreground", "deliver"]
    );
    assert_eq!(r.world().delivered.len(), 1);
}

#[test]
fn target_window_rechecks_age_after_the_last_foreground_read() {
    let (r, result) = click_expiring_during_the_last_foreground_read(ActionFreshness::TargetWindow);
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));
    assert_eq!(r.world().events, ["foreground", "capture", "foreground"]);
    assert!(
        r.world().delivered.is_empty(),
        "nothing delivered after expiry"
    );
}

#[test]
fn unavailable_group_reads_without_a_known_change_keep_the_whole_frame_fallback() {
    for sequence in [[0, 0, 1], [0, 1, 1], [1, 0, 1], [1, 1, 1]] {
        let mut r = scoped_rig();
        let page = r.observe();
        let observed = r.world().top_windows.clone();
        r.world()
            .top_windows_script
            .extend(sequence.map(|read| if read == 0 { observed.clone() } else { None }));
        let result =
            r.engine
                .execute_action(&live_click(&page, "unknown-only", ExpectedEffect::None));
        assert_eq!(
            result.status,
            ActionStatus::InputDelivered,
            "{sequence:?}: {result:?}"
        );
    }
}

#[test]
fn a_known_group_change_spends_the_snapshot_even_when_the_capture_fails() {
    let mut r = scoped_rig();
    let page = r.observe();
    let mut changed = r.world().top_windows.clone();
    changed.as_mut().unwrap()[0].z_order += 1;
    r.world().top_windows_script.push_back(changed);
    r.world().fail_capture = true;
    let result =
        r.engine
            .execute_action(&live_click(&page, "failed-capture", ExpectedEffect::None));
    assert_eq!(error_code(&result), Some(ErrorCode::StaleSnapshot));
    assert!(r.world().delivered.is_empty(), "nothing sent");
    assert_eq!(
        r.engine.last_action_timings().unwrap().freshness_diff,
        None,
        "no frame to count"
    );
    r.world().fail_capture = false;
    r.world().top_windows_script.clear();
    let retry = r.engine.execute_action(&live_click(
        &page,
        "spent-after-capture",
        ExpectedEffect::None,
    ));
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    assert!(
        r.world().delivered.is_empty(),
        "the known change spent the snapshot"
    );
}
