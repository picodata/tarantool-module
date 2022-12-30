use std::collections::HashMap;
use tarantool::c_ptr;
use tarantool::ffi::tarantool as ffi;
use tarantool::index::{Index, IteratorType};
use tarantool::space::Space;
use tarantool::tuple::Tuple;

pub fn space_ephemeral_new() {
    unsafe {
        let space_def = Tuple::new(&(
            314,
            1,
            "ass_face",
            "memtx",
            0,
            HashMap::<(), ()>::new(),
            vec![
                HashMap::from([("name", "id"), ("type", "unsigned")]),
                HashMap::from([("name", "value"), ("type", "any")]),
            ],
        ))
        .unwrap();
        let s = ffi::pico_space_ephemeral_new(std::mem::transmute_copy(&space_def));
        assert!(!s.is_null());
        let space_id = ffi::pico_space_id(s);
        assert_eq!(space_id, 314);

        let pk = ffi::pico_space_index(s, 0);
        assert!(pk.is_null());

        let opts = rmp_serde::to_vec_named(&HashMap::from([("unique", true)])).unwrap();
        let parts = rmp_serde::to_vec(&[(0, "unsigned")]).unwrap();
        let i = ffi::pico_space_ephemeral_index_new(
            s,
            c_ptr!("pk"),
            2,
            c_ptr!("TREE"),
            4,
            opts.as_ptr() as _,
            parts.as_ptr() as _,
        );
        assert!(!i.is_null());
        let index_id = ffi::pico_index_id(i);
        assert_eq!(index_id, 0);
        assert_eq!(ffi::pico_space_index(s, index_id), i);

        let space: Space = std::mem::transmute(space_id);
        space.insert(&(13, "hello")).unwrap();
        space.insert(&(37, "friend")).unwrap();

        let rows = space
            .select(IteratorType::All, &())
            .unwrap()
            .map(|t| t.decode::<(i32, String)>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            [(13, "hello".to_string()), (37, "friend".to_string())]
        );

        let parts = rmp_serde::to_vec(&[(1, "string")]).unwrap();
        let i = ffi::pico_space_ephemeral_index_new(
            s,
            c_ptr!("value"),
            2,
            c_ptr!("HASH"),
            4,
            opts.as_ptr() as _,
            parts.as_ptr() as _,
        );
        assert!(!i.is_null());
        let index_id = ffi::pico_index_id(i);
        assert_eq!(index_id, 1);
        assert_eq!(ffi::pico_space_index(s, index_id), i);

        let index: Index = std::mem::transmute((space_id, index_id));
        assert!(index.get(&["not-found"]).unwrap().is_none());
        let t = index.get(&["friend"]).unwrap().unwrap();
        assert_eq!(t.get::<_, i32>(0).unwrap(), 37);

        ffi::pico_space_ephemeral_delete(s);
    }
}
