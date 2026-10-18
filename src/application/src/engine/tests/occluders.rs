//! ADR-0035: under `target_window` the freshness group is the target window plus the visible
//! windows above it that overlap it (grown by the margin). Windows below it are not in the
//! group; the foreground window is a member only by that rule, and its handle is a separate
//! identity. The target window is `T` (handle 1, rectangle 64,64..576,320) and the Save
//! button (400,250..480,280) lies well inside it.

use super::freshness::{FAR_PIXEL, assert_stale_and_nothing_sent, scoped_rig, scoped_window};
use super::*;
use crate::contract::FreshnessDiff;

const WHITE: [u8; 3] = [255, 255, 255];

/// The target window, at `z_order`.
fn target(z_order: u32) -> TopLevelWindow {
    scoped_window(1, z_order, bx(64.0, 64.0, 576.0, 320.0))
}

/// A rig whose windows are `windows` (T among them, handle 1) and whose foreground window is
/// the one with handle `foreground`. The window under the click is always T.
fn rig_with_windows(windows: Vec<TopLevelWindow>, foreground: u64) -> Rig {
    let r = scoped_rig();
    let at = |handle: u64| {
        windows
            .iter()
            .find(|w| w.handle == handle)
            .cloned()
            .expect("a listed window")
    };
    let (target_window, foreground_window) = (at(1), at(foreground));
    {
        let mut world = r.world();
        world.window_at_point = Some(target_window);
        world.foreground_window = Some(foreground_window);
        world.top_windows = Some(windows);
    }
    r
}

fn click(r: &mut Rig, page: &ObservePage) -> ActionResult {
    r.engine
        .execute_action(&live_click(page, "r", ExpectedEffect::None))
}

fn delivered(result: &ActionResult) {
    assert_eq!(result.status, ActionStatus::InputDelivered, "{result:?}");
}

fn diagnostic(r: &Rig) -> FreshnessDiff {
    r.engine
        .last_action_timings()
        .unwrap()
        .freshness_diff
        .unwrap()
}

/// A window above T that overlaps it (its top-right corner).
fn overlay(handle: u64, z_order: u32) -> TopLevelWindow {
    scoped_window(handle, z_order, bx(500.0, 0.0, 640.0, 100.0))
}

/// UT-65: a window below T is not in the group, so its pixels and its moves do not refuse, even
/// when it overlaps T. The same window raised above T does.
#[test]
fn ut_65_a_change_in_a_window_below_the_target_is_accepted_and_above_it_refuses() {
    let below = || scoped_window(2, 1, bx(500.0, 0.0, 640.0, 100.0));
    let mut r = rig_with_windows(vec![target(0), below()], 1);
    let page = r.observe();
    r.world().pixels.push((630, 10, WHITE));
    delivered(&click(&mut r, &page));

    // It moves while still below T: accepted.
    let mut r = rig_with_windows(vec![target(0), below()], 1);
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap()[1].rectangle = Some(bx(480.0, 0.0, 640.0, 120.0));
    delivered(&click(&mut r, &page));

    // Raised above T it enters the group: a pixel change and an unchanged-pixel raise refuse.
    let raised = || vec![scoped_window(2, 0, bx(500.0, 0.0, 640.0, 100.0)), target(1)];
    let mut r = rig_with_windows(vec![target(0), below()], 1);
    let page = r.observe();
    r.world().top_windows = Some(raised());
    r.world().pixels.push((630, 10, WHITE));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    let mut r = rig_with_windows(vec![target(0), below()], 1);
    let page = r.observe();
    r.world().top_windows = Some(raised());
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

/// UT-66: the group is T and the windows above it that overlap it. A window above T that
/// overlaps nothing, and the foreground window, are members only by that rule.
#[test]
fn ut_66_the_group_is_the_target_and_the_overlapping_windows_above_it() {
    let apart = scoped_window(3, 0, bx(0.0, 0.0, 40.0, 40.0));
    let windows = || vec![apart.clone(), overlay(2, 1), target(2)];
    // The foreground window is `apart`, above T and overlapping nothing: not a member.
    let mut r = rig_with_windows(windows(), 3);
    let page = r.observe();
    r.world().pixels.push((10, 10, WHITE));
    delivered(&click(&mut r, &page));

    // The overlay above T is a member: its pixels refuse.
    let mut r = rig_with_windows(windows(), 3);
    let page = r.observe();
    r.world().pixels.push((630, 10, WHITE));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    assert_eq!(diagnostic(&r).group_tiles, Some(1));
}

/// UT-66: the group is ordered by z-order then handle, whatever order the OS list arrives in.
#[test]
fn ut_66_the_group_order_is_z_order_then_handle() {
    let mut r = rig_with_windows(vec![overlay(3, 0), overlay(2, 0), target(1)], 1);
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap().reverse();
    delivered(&click(&mut r, &page));
}

/// UT-67: any order change between two members, any member entering or leaving, and any
/// rectangle change refuse with the pixels identical.
#[test]
fn ut_67_an_order_swap_between_two_members_refuses() {
    let second = || scoped_window(3, 1, bx(480.0, 0.0, 640.0, 90.0));
    let mut r = rig_with_windows(vec![overlay(2, 0), second(), target(2)], 1);
    let page = r.observe();
    r.world().top_windows = Some(vec![
        scoped_window(3, 0, bx(480.0, 0.0, 640.0, 90.0)),
        overlay(2, 1),
        target(2),
    ]);
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn ut_67_a_member_leaving_the_group_refuses() {
    let mut r = rig_with_windows(vec![overlay(2, 0), target(1)], 1);
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap()[0].rectangle = Some(bx(590.0, 0.0, 640.0, 100.0));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn ut_67_a_window_above_that_moves_into_an_overlap_refuses() {
    let apart = || scoped_window(2, 0, bx(590.0, 0.0, 640.0, 100.0));
    let mut r = rig_with_windows(vec![apart(), target(1)], 1);
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap()[0].rectangle = Some(bx(500.0, 0.0, 640.0, 100.0));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

#[test]
fn ut_67_a_rectangle_change_of_a_member_refuses_without_a_pixel_change() {
    let mut r = rig_with_windows(vec![overlay(2, 0), target(1)], 1);
    let page = r.observe();
    r.world().top_windows.as_mut().unwrap()[0].rectangle = Some(bx(501.0, 0.0, 640.0, 100.0));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

/// A window above T that overlaps nothing is not in the group, so its disappearing shifts T's
/// place in the OS list without being a change of the group.
#[test]
fn ut_67_a_window_above_that_overlaps_nothing_may_close() {
    let apart = scoped_window(3, 0, bx(0.0, 0.0, 40.0, 40.0));
    let mut r = rig_with_windows(vec![apart, target(1)], 1);
    let page = r.observe();
    *r.world().top_windows.as_mut().unwrap() = vec![target(0)];
    delivered(&click(&mut r, &page));
}

/// UT-68: a window that rises above T between the two looks around the capture refuses.
#[test]
fn ut_68_a_window_rising_above_the_target_between_the_looks_refuses() {
    let below = scoped_window(2, 1, bx(500.0, 0.0, 640.0, 100.0));
    let mut r = rig_with_windows(vec![target(0), below], 1);
    let page = r.observe();
    let original = r.world().top_windows.clone();
    let raised = Some(vec![
        scoped_window(2, 0, bx(500.0, 0.0, 640.0, 100.0)),
        target(1),
    ]);
    r.world()
        .top_windows_script
        .extend([original, raised.clone(), raised]);
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    let retry = click(&mut r, &page);
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
}

/// UT-69: a change of the foreground window handle refuses with `STALE_SNAPSHOT` on identical
/// pixels and an identical group, with the same application and no window scope configured.
#[test]
fn ut_69_a_foreground_handle_change_refuses_on_identical_pixels_and_group() {
    let other = scoped_window(7, 1, bx(0.0, 200.0, 40.0, 320.0));
    let mut r = rig_with_windows(vec![target(0), other.clone()], 1);
    let page = r.observe();
    r.world().foreground_window = Some(other);
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    assert_eq!(diagnostic(&r).total_tiles, 0, "identical frame bytes");
}

/// UT-70: a target window the actuator cannot attest opaque (layered, click-through,
/// translucent, a backdrop, or any failed query) gives the same result as `screen`.
#[test]
fn ut_70_an_unattested_opacity_compares_the_whole_frame() {
    let mut r = scoped_rig();
    let page = r.observe();
    r.world().opacity = None;
    r.world().pixels.push((FAR_PIXEL.0, FAR_PIXEL.1, WHITE));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    assert_eq!(diagnostic(&r).group_tiles, None);
}

#[test]
fn ut_70_losing_the_opacity_attestation_at_the_last_look_compares_the_whole_frame() {
    let mut r = scoped_rig();
    let page = r.observe();
    let opaque = Some(WindowOpacity {
        corner_exclusion_px: 0,
    });
    r.world().opacity_script.extend([opaque, opaque, None]);
    r.world().pixels.push((FAR_PIXEL.0, FAR_PIXEL.1, WHITE));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

/// UT-71: a target box that meets a corner square of T is not attested opaque, and one clear of
/// the four squares keeps the scope.
#[test]
fn ut_71_a_box_touching_a_rounded_corner_compares_the_whole_frame() {
    // The Save button is 96 px from the right edge and 40 px from the bottom edge of T: it
    // meets the bottom-right corner square only when the radius exceeds both distances.
    for (radius, scoped) in [(32, true), (96, true), (97, false), (100, false)] {
        let mut r = scoped_rig();
        r.world().opacity = Some(WindowOpacity {
            corner_exclusion_px: radius,
        });
        let page = r.observe();
        r.world().pixels.push((FAR_PIXEL.0, FAR_PIXEL.1, WHITE));
        let result = click(&mut r, &page);
        if scoped {
            delivered(&result);
        } else {
            assert_stale_and_nothing_sent(&r, &result);
            assert_eq!(diagnostic(&r).group_tiles, None, "radius {radius}");
        }
    }
}

/// UT-72: shadow pixels inside T refuse at margin 0: the exact comparison covers all of T.
#[test]
fn ut_72_changed_shadow_pixels_inside_the_target_refuse_at_margin_zero() {
    let caster = scoped_window(2, 0, bx(580.0, 100.0, 640.0, 150.0));
    let mut r = rig_with_windows(vec![caster, target(1)], 1);
    r.engine.config.action_freshness_margin_px = 0;
    let page = r.observe();
    r.world().pixels.push((570, 100, [20, 20, 20]));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
}

/// UT-73: the casting window's identity enters the list only when its bounds intersect T's
/// rectangle grown by the margin (the casting window is 4 px from T).
#[test]
fn ut_73_the_casting_window_enters_the_list_only_inside_the_expanded_region() {
    for (margin, enters) in [(0, false), (3, false), (4, false), (5, true), (64, true)] {
        let caster = scoped_window(2, 0, bx(580.0, 100.0, 640.0, 150.0));
        let mut r = rig_with_windows(vec![caster, target(1)], 1);
        r.engine.config.action_freshness_margin_px = margin;
        let page = r.observe();
        r.world().top_windows.as_mut().unwrap()[0].rectangle = Some(bx(581.0, 100.0, 640.0, 150.0));
        let result = click(&mut r, &page);
        if enters {
            assert_stale_and_nothing_sent(&r, &result);
        } else {
            delivered(&result);
        }
    }
}

/// UT-74: over generated frames, windows and margins, the amended scope accepts exactly when
/// no changed tile meets T or an overlapping window above it, and at margin 0 it refuses on
/// pixels only where the ADR-0032 group does. The expectation is independent scalar interval arithmetic.
#[test]
fn ut_74_the_amended_scope_refuses_on_pixels_only_where_adr_0032_does() {
    type Rect = (f64, f64, f64, f64);
    let hit = |rect: Rect, margin: f64, left: f64, top: f64| {
        left < rect.2 + margin
            && left + 64.0 > rect.0 - margin
            && top < rect.3 + margin
            && top + 64.0 > rect.1 - margin
    };
    let overlaps = |a: Rect, b: Rect, margin: f64| {
        a.0 < b.2 + margin && a.2 > b.0 - margin && a.1 < b.3 + margin && a.3 > b.1 - margin
    };
    let t: Rect = (64.0, 64.0, 576.0, 320.0);
    let above: Rect = (500.0, 0.0, 640.0, 100.0);
    let apart: Rect = (0.0, 0.0, 40.0, 40.0);
    let below: Rect = (0.0, 200.0, 200.0, 320.0);
    let all = [("above", above), ("apart", apart), ("below", below)];
    for margin in [0u32, 1, 64] {
        let m = f64::from(margin);
        for row in 0..5u32 {
            for col in 0..10u32 {
                let x = col * 64 + (col * 17 + row * 7) % 64;
                let y = row * 64 + (col * 11 + row * 3) % 64;
                let mut r = rig_with_windows(
                    vec![
                        scoped_window(3, 0, bx(apart.0, apart.1, apart.2, apart.3)),
                        scoped_window(2, 1, bx(above.0, above.1, above.2, above.3)),
                        target(2),
                        scoped_window(4, 3, bx(below.0, below.1, below.2, below.3)),
                    ],
                    3,
                );
                r.engine.config.action_freshness_margin_px = margin;
                let page = r.observe();
                r.world().pixels.push((x, y, WHITE));
                let (left, top) = (f64::from(col * 64), f64::from(row * 64));
                let members: Vec<Rect> = std::iter::once(t)
                    .chain(
                        all.iter()
                            .filter(|(name, rect)| *name != "below" && overlaps(*rect, t, m))
                            .map(|(_, rect)| *rect),
                    )
                    .collect();
                let amended = members.iter().any(|&rect| hit(rect, m, left, top));
                // ADR-0032: T, the foreground window, and every window overlapping either.
                let old: Vec<Rect> = std::iter::once(t)
                    .chain(std::iter::once(apart))
                    .chain(
                        all.iter()
                            .filter(|(_, rect)| {
                                overlaps(*rect, t, 0.0) || overlaps(*rect, apart, 0.0)
                            })
                            .map(|(_, rect)| *rect),
                    )
                    .collect();
                let adr_32 = old.iter().any(|&rect| hit(rect, m, left, top));
                // The subset holds at margin 0 only: a positive margin can add a window that
                // ADR-0032 excludes (a conservative extension, see the next tests).
                assert!(
                    margin != 0 || !amended || adr_32,
                    "pixel {x},{y}; margin {margin}"
                );
                let result = click(&mut r, &page);
                assert_eq!(
                    error_code(&result),
                    amended.then_some(ErrorCode::StaleSnapshot),
                    "pixel {x},{y}; margin {margin}"
                );
                assert_eq!(r.world().delivered.len(), usize::from(!amended));
            }
        }
    }
}

/// UT-70 (review 2, F3): when the opacity cannot be attested, an identity refusal keeps the
/// refusal and spends the snapshot but reports no group: the scope was not available.
#[test]
fn ut_70_an_identity_refusal_without_attested_opacity_omits_the_group_count() {
    let other = || scoped_window(7, 1, bx(0.0, 200.0, 40.0, 320.0));
    // 0: the actuator never attests; 1: it attests twice and loses it at the last look.
    for loses_at_the_last_look in [false, true] {
        let mut r = rig_with_windows(vec![target(0), other()], 1);
        let page = r.observe();
        r.world().foreground_window = Some(other());
        if loses_at_the_last_look {
            let opaque = Some(WindowOpacity {
                corner_exclusion_px: 0,
            });
            r.world().opacity_script.extend([opaque, opaque, None]);
        } else {
            r.world().opacity = None;
        }
        let result = click(&mut r, &page);
        assert_stale_and_nothing_sent(&r, &result);
        let d = diagnostic(&r);
        assert_eq!(
            (d.total_tiles, d.group_tiles),
            (0, None),
            "last look {loses_at_the_last_look}"
        );
        let retry = click(&mut r, &page);
        assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    }
}

/// UT-74 (review 2, F4): with a positive margin ADR-0035 can include a window that ADR-0032
/// excludes (a conservative extension: more refusals, never fewer). T is `[300,64,500,320)`
/// and the window above it `[504,100,640,150)`; at margin 5 only the expanded T reaches it.
#[test]
fn ut_74_a_window_that_enters_only_through_the_margin_refuses_where_adr_0032_accepts() {
    let t = || scoped_window(1, 1, bx(300.0, 64.0, 500.0, 320.0));
    let w = || scoped_window(2, 0, bx(504.0, 100.0, 640.0, 150.0));
    for (margin, refuses) in [(0, false), (3, false), (4, false), (5, true)] {
        let mut r = rig_with_windows(vec![w(), t()], 1);
        r.engine.config.action_freshness_margin_px = margin;
        let page = r.observe();
        r.world().pixels.push((600, 110, WHITE));
        let result = click(&mut r, &page);
        if refuses {
            assert_stale_and_nothing_sent(&r, &result);
        } else {
            delivered(&result);
        }
    }
}

/// UT-74 (review 2, F4): deterministic layouts (rectangles and relative z-orders, with windows
/// that enter only through the margin) against an independent scalar reference. T is
/// `[300,64,500,320)` and is the foreground; at margin 0 the group is a subset of ADR-0032's.
#[test]
fn ut_74_generated_layouts_match_the_scalar_reference_for_every_margin() {
    type Rect = (f64, f64, f64, f64);
    let to_box = |r: Rect| bx(r.0, r.1, r.2, r.3);
    let t: Rect = (300.0, 64.0, 500.0, 320.0);
    let overlaps =
        |a: Rect, b: Rect, m: f64| a.0 < b.2 + m && a.2 > b.0 - m && a.1 < b.3 + m && a.3 > b.1 - m;
    let tile_hit = |r: Rect, m: f64, col: u32, row: u32| {
        let (left, top) = (f64::from(col * 64), f64::from(row * 64));
        left < r.2 + m && left + 64.0 > r.0 - m && top < r.3 + m && top + 64.0 > r.1 - m
    };
    for margin in [0u32, 1, 5, 64] {
        let m = f64::from(margin);
        for layout in 0..16u32 {
            // Up to four other windows, handles 2.., and T somewhere in the relative order.
            let n = 1 + layout % 4;
            let others: Vec<Rect> = (0..n)
                .map(|j| {
                    let x0 = f64::from((layout * 37 + j * 53) % 560);
                    let y0 = f64::from((layout * 17 + j * 71) % 250);
                    let w = f64::from(40 + (layout * 13 + j * 29) % 120);
                    let h = f64::from(30 + (layout * 7 + j * 19) % 100);
                    (x0, y0, (x0 + w).min(640.0), (y0 + h).min(320.0))
                })
                .collect();
            let t_position = (layout % (n + 1)) as usize;
            let mut listed: Vec<TopLevelWindow> = Vec::new();
            let mut other = others.iter().zip(2u64..);
            for position in 0..=others.len() {
                let z = u32::try_from(position).unwrap();
                if position == t_position {
                    listed.push(scoped_window(1, z, to_box(t)));
                } else if let Some((rect, handle)) = other.next() {
                    listed.push(scoped_window(handle, z, to_box(*rect)));
                }
            }
            // Members: T, and the windows listed before T that overlap T grown by the margin.
            let members: Vec<Rect> = std::iter::once(t)
                .chain(
                    listed
                        .iter()
                        .take(t_position)
                        .map(|w| w.rectangle.unwrap())
                        .map(|b| (b.x0(), b.y0(), b.x1(), b.y1()))
                        .filter(|&r| overlaps(r, t, m)),
                )
                .collect();
            let adr_32: Vec<Rect> = std::iter::once(t)
                .chain(
                    listed
                        .iter()
                        .map(|w| w.rectangle.unwrap())
                        .map(|b| (b.x0(), b.y0(), b.x1(), b.y1()))
                        .filter(|&r| overlaps(r, t, 0.0)),
                )
                .collect();
            for k in 0..10u32 {
                let x = (layout * 89 + k * 61 + 7) % 640;
                let y = (layout * 43 + k * 37 + 5) % 320;
                let (col, row) = (x / 64, y / 64);
                let expect = members.iter().any(|&r| tile_hit(r, m, col, row));
                let old = adr_32.iter().any(|&r| tile_hit(r, m, col, row));
                assert!(
                    margin != 0 || !expect || old,
                    "subset at margin 0: layout {layout} pixel {x},{y}"
                );
                let mut r = rig_with_windows(listed.clone(), 1);
                r.engine.config.action_freshness_margin_px = margin;
                let page = r.observe();
                r.world().pixels.push((x, y, WHITE));
                let result = click(&mut r, &page);
                assert_eq!(
                    error_code(&result),
                    expect.then_some(ErrorCode::StaleSnapshot),
                    "layout {layout}; pixel {x},{y}; margin {margin}"
                );
            }
        }
    }
}

/// UT-70 (review 2, F1/F2): whichever look loses the opacity attestation (a failed style
/// query, or a window composed without a redirection bitmap, makes the adapter answer
/// `None`), the whole frame is compared and a far changed pixel refuses.
#[test]
fn ut_70_losing_the_opacity_at_any_look_takes_the_whole_frame_path() {
    let opaque = Some(WindowOpacity {
        corner_exclusion_px: 0,
    });
    for lost_at in 0..3 {
        let mut r = scoped_rig();
        let page = r.observe();
        let mut reads = [opaque, opaque, opaque];
        reads[lost_at] = None;
        r.world().opacity_script.extend(reads);
        r.world().pixels.push((FAR_PIXEL.0, FAR_PIXEL.1, WHITE));
        let result = click(&mut r, &page);
        assert_stale_and_nothing_sent(&r, &result);
        assert_eq!(diagnostic(&r).group_tiles, None, "lost at look {lost_at}");
    }
}

/// UT-70 (review 3, R1): the whole-frame diagnostic scan does not depend on whether the group
/// may be reported. A changed external pixel, a foreground identity change and the loss of the
/// opacity at the last look refuse with the global count and no group count.
#[test]
fn ut_70_an_identity_refusal_after_a_regional_comparison_still_counts_the_whole_frame() {
    let other = || scoped_window(2, 1, bx(0.0, 200.0, 40.0, 320.0));
    let mut r = rig_with_windows(vec![target(0), other()], 1);
    let page = r.observe();
    let opaque = Some(WindowOpacity {
        corner_exclusion_px: 0,
    });
    // Two foreground queries per group read: the first two reads keep T in front, the last
    // one reports the other window.
    let front = Some(target(0));
    r.world().foreground_window_script.extend([
        front.clone(),
        front.clone(),
        front.clone(),
        front,
        Some(other()),
        Some(other()),
    ]);
    r.world().opacity_script.extend([opaque, opaque, None]);
    r.world().pixels.push((FAR_PIXEL.0, FAR_PIXEL.1, WHITE));
    let result = click(&mut r, &page);
    assert_stale_and_nothing_sent(&r, &result);
    let d = diagnostic(&r);
    assert_eq!((d.total_tiles, d.group_tiles), (1, None));
    let retry = click(&mut r, &page);
    assert_eq!(error_code(&retry), Some(ErrorCode::StaleSnapshot));
    assert!(r.world().delivered.is_empty(), "the snapshot stays spent");
}
