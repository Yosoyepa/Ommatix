"""Regresiones reproducibles. Sin escritorio, sin modelos, sin entrada del SO."""
import unittest
import itertools
from oraculo.core import *


def blank(w=65,h=65,padding=0,epoch=1):
    stride=4*w+padding
    return Frame(w,h,stride,bytes(stride*h),epoch)


def changed(frame,x,y,channel=0):
    buf=bytearray(frame.pixels);buf[y*frame.stride+4*x+channel]=255
    return Frame(frame.width,frame.height,frame.stride,bytes(buf),frame.epoch)

class CoreTests(unittest.TestCase):
    def test_or01_ioc_small_text(self):
        b=Rect(100,100,400,140);t=Rect(235,112,260,126)
        self.assertEqual(b.ioc(t),1);self.assertLess(t.area/b.area,.03)
    def test_or02_reject_nan(self):
        with self.assertRaises(ValueError):Rect(float('nan'),0,1,1)
    def test_or03_reject_empty_rect(self):
        with self.assertRaises(ValueError):Rect(0,0,0,2)
    def test_or04_first_frame_is_dirty(self):
        self.assertEqual(len(dirty_tiles(None,blank())),4)
    def test_or05_identical_frames(self):
        f=blank();self.assertEqual(dirty_tiles(f,f),set())
    def test_or06_single_pixel_odd_row(self):
        f=blank();self.assertEqual(dirty_tiles(f,changed(f,10,1)),{(0,0)})
    def test_or07_odd_width_last_pixel(self):
        f=blank();self.assertEqual(dirty_tiles(f,changed(f,64,64)),{(1,1)})
    def test_or08_alpha_ignored(self):
        f=blank();self.assertEqual(dirty_tiles(f,changed(f,1,1,3)),set())
    def test_or09_stride_padding_ignored(self):
        f=blank(padding=8);buf=bytearray(f.pixels);buf[4*f.width]=1
        g=Frame(f.width,f.height,f.stride,bytes(buf));self.assertEqual(dirty_tiles(f,g),set())
    def test_or10_stride_real_pixel(self):
        f=blank(padding=12);self.assertEqual(dirty_tiles(f,changed(f,4,63)),{(0,0)})
    def test_or11_invalid_buffer(self):
        with self.assertRaises(ValueError):Frame(2,2,8,b'bad')
    def test_or12_epoch_change(self):
        f=blank();g=Frame(f.width,f.height,f.stride,f.pixels,2)
        self.assertEqual(len(dirty_tiles(f,g)),4)
    def test_or13_invalid_tile_size(self):
        with self.assertRaises(ValueError):dirty_tiles(None,blank(),0)
    def test_or14_halo_transitive(self):
        r=Rect(0,0,10,10);a=Rect(8,0,20,10);b=Rect(18,0,30,10)
        self.assertEqual(halo_close(r,[b,a],Rect(0,0,100,100)),Rect(0,0,30,10))
    def test_or15_halo_order_invariant(self):
        r=Rect(0,0,10,10);s=[Rect(8,0,20,10),Rect(18,0,30,10)];bounds=Rect(0,0,100,100)
        self.assertEqual(halo_close(r,s,bounds),halo_close(r,list(reversed(s)),bounds))
    def test_or16_halo_budget_fallback(self):
        bounds=Rect(0,0,100,100)
        self.assertEqual(halo_close(Rect(0,0,10,10),[Rect(8,0,20,10)],bounds,0),bounds)
    def test_or17_merge_transitive(self):
        a=Rect(0,0,10,10);b=Rect(8,0,20,10);c=Rect(18,0,30,10)
        self.assertEqual(merge_regions([c,a,b]),[Rect(0,0,30,10)])
    def test_or18_stage_does_not_mutate(self):
        store=SnapshotStore();p=store.stage(blank(),['e1'])
        self.assertIsNone(store.frame);self.assertEqual(store.revision,0)
        store.commit(p);self.assertEqual(store.payload,('e1',))
    def test_or19_failed_inference_keeps_dirty(self):
        store=SnapshotStore();f=blank();store.commit(store.stage(f,['e1']))
        g=changed(f,0,1);store.stage(g,['e2']) # no commit: inferencia abortada
        self.assertEqual(store.payload,('e1',));self.assertEqual(dirty_tiles(store.frame,g),{(0,0)})
    def test_or20_stale_commit_rejected(self):
        store=SnapshotStore();a=store.stage(blank(),['a']);b=store.stage(blank(),['b']);store.commit(a)
        with self.assertRaisesRegex(ValueError,'STALE_COMMIT'):store.commit(b)
    def test_or21_reading_order(self):
        words=[Word('Sesión',Rect(30,10,60,20)),Word('Iniciar',Rect(0,10,28,20))]
        self.assertEqual(' '.join(w.text for w in reading_order(words)),'Iniciar Sesión')
    def test_or22_reading_permutation_invariant(self):
        words=[Word('A',Rect(20,0,25,10)),Word('B',Rect(10,4,15,14)),Word('C',Rect(0,8,5,18))]
        expected=reading_order(words)
        for sequence in itertools.permutations(words):self.assertEqual(reading_order(sequence),expected)
    def test_or23_specific_owner(self):
        panel=Rect(0,0,200,200);button=Rect(10,10,100,40);word=Word('OK',Rect(20,15,30,25))
        result=internal_owners([panel,button],[word]);self.assertEqual(result[0],[]);self.assertEqual(result[1],[word])
    def test_or24_owner_tie_abstains(self):
        box=Rect(0,0,100,100);w=Word('OK',Rect(10,10,30,20))
        self.assertEqual(internal_owners([box,box],[w]),{0:[],1:[]})
    def test_or25_unicode_and_word_boundary(self):
        self.assertEqual(normalized(' SEÑOR  '),normalized('sen\u0303or'))
        self.assertNotEqual(normalized('a b'),normalized('ab'))
    def test_or26_corridor_excludes_endpoints(self):
        self.assertEqual(corridor(Rect(0,0,10,10),Rect(20,0,50,10)),Rect(10,0,20,10))
    def test_or27_external_label(self):
        a=Rect(100,100,300,135);b=Rect(350,100,550,135)
        words=[Word('Nombre',Rect(30,105,90,128)),Word('Apellido',Rect(350,72,430,92))]
        self.assertEqual(choose_external_label(a,words,[a,b]),'Nombre')
        self.assertEqual(choose_external_label(b,words,[a,b]),'Apellido')
    def test_or28_external_tie_abstains(self):
        b=Rect(100,100,200,140);t=Rect(100,70,150,90)
        self.assertIsNone(choose_external_label(b,[Word('A',t),Word('B',t)],[]))
    def test_or29_legacy_theme_counterexample(self):
        row=[0,10,10,0,0,10,10,0,0];a=row*8;b=[255-v for v in a]
        self.assertEqual(legacy_distance(legacy_dhash(a),legacy_dhash(b)),32)
    def test_or30_masked_inversion(self):
        a=[0,10,10,0,0,10,10,0,0]*8;b=[255-v for v in a]
        self.assertEqual(descriptor_distance(gradient_descriptor(a),gradient_descriptor(b)),0)
    def test_or31_flat_descriptor_rejected(self):
        a=gradient_descriptor([0]*72);self.assertIsNone(descriptor_distance(a,a))
    def test_or32_bad_icon_shape(self):
        with self.assertRaises(ValueError):gradient_descriptor([0]*71)
    def test_or33_low_support_rejected(self):
        a=GradientDescriptor(1,1);self.assertIsNone(descriptor_distance(a,a))
    def test_or34_grid_boundary_changes_id(self):
        self.assertNotEqual(legacy_id(1,'Guardar',63,100),legacy_id(1,'Guardar',64,100))
    def test_or35_duplicate_seed_collides(self):
        self.assertEqual(legacy_id(1,'Eliminar',10,10),legacy_id(1,'Eliminar',20,20))
    def test_or36_birthday_probability(self):
        self.assertGreater(collision_probability(300),.39);self.assertLess(collision_probability(300),.40)
    def test_or37_registry_not_position_hash(self):
        registry=Registry();e=registry.add(Rect(60,0,66,10),'Guardar','confirmed')
        moved=registry.update(e.target_id,bbox=Rect(61,0,67,10))
        self.assertEqual(moved.target_id,e.target_id);self.assertEqual(moved.revision,2)
    def test_or38_registry_no_reuse(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A');r.retire(e.target_id)
        self.assertNotEqual(e.target_id,r.add(Rect(0,0,10,10),'A').target_id)
    def test_or39_stale_snapshot_rejected(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');s=r.snapshot;r.add(Rect(10,10,20,20),'B')
        with self.assertRaisesRegex(ValueError,'STALE_SNAPSHOT'):r.prepare(r.session_id,s,e.target_id,1)
    def test_or40_revision_rejected(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');r.update(e.target_id,label='B')
        with self.assertRaisesRegex(ValueError,'STALE_TARGET'):r.prepare(r.session_id,r.snapshot,e.target_id,1)
    def test_or41_orphan_not_actionable(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'Texto')
        with self.assertRaisesRegex(ValueError,'NOT_ACTIONABLE'):r.prepare(r.session_id,r.snapshot,e.target_id,1)
    def test_or42_wrong_session(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed')
        with self.assertRaisesRegex(ValueError,'SESSION_MISMATCH'):r.prepare('other',r.snapshot,e.target_id,1)
    def test_or43_never_reports_real_execution(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');res=r.prepare(r.session_id,r.snapshot,e.target_id,1)
        self.assertEqual(res['status'],'simulated');self.assertFalse(res['input_delivered'])
    def test_or44_real_actuation_not_implemented(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed')
        with self.assertRaisesRegex(ValueError,'ACTUATION_NOT_IMPLEMENTED'):r.prepare(r.session_id,r.snapshot,e.target_id,1,dry_run=False)
    def test_or45_disabled_rejected(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');e=r.update(e.target_id,enabled=False)
        with self.assertRaisesRegex(ValueError,'NOT_ACTIONABLE'):r.prepare(r.session_id,r.snapshot,e.target_id,e.revision)
    def test_or46_hidden_rejected(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');e=r.update(e.target_id,visible=False)
        with self.assertRaisesRegex(ValueError,'NOT_ACTIONABLE'):r.prepare(r.session_id,r.snapshot,e.target_id,e.revision)
    def test_or47_retired_missing(self):
        r=Registry();e=r.add(Rect(0,0,10,10),'A','confirmed');r.retire(e.target_id)
        with self.assertRaisesRegex(ValueError,'TARGET_NOT_FOUND'):r.prepare(r.session_id,r.snapshot,e.target_id,1)
    def test_or48_all_small_pixels_detected(self):
        f=blank(9,7,8)
        for y in range(f.height):
            for x in range(f.width):self.assertEqual(dirty_tiles(f,changed(f,x,y),4),{(x//4,y//4)})

if __name__=='__main__':unittest.main()
