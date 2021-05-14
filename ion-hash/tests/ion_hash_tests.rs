// Copyright Amazon.com, Inc. or its affiliates.

use digest::{consts::U256, generic_array::GenericArray, Digest, Output};
use ion_hash::{self, IonHasher};
use ion_rs::result::IonResult;
use ion_rs::value::reader::{element_reader, ElementReader};
use ion_rs::value::*;
use std::fs::read;

// This type exists purely to facilitate testing with ion-hash-test. See that
// package for details on how the tests are structured.
//
// In a nutshell, the purpose of the tests are to ensure that the Ion Hash
// implementation correctly normalizes and represents the Ion values. The
// purpose is _not_ to test the hashing function! So.. `updates` track the byte
// representations of values we incorporate into the hash.
//
// The `Digest` trait specifies a fixed length output. We pick a `N` that is
// bigger than the largest "identity hash" test case. Note that this means tests
// may have to deal with trailing zeros (e.g. the identity hash might be 56
// bytes long, with 200 trailing zeros in the fixed-length array).
#[derive(Default, Clone)]
struct TestDigest {
    updates: GenericArray<u8, U256>,
    position: usize,
}

impl Digest for TestDigest {
    // Pick a number bigger than the biggest test case digest expectation.
    type OutputSize = U256;

    fn new() -> Self {
        Self {
            updates: GenericArray::default(),
            position: 0,
        }
    }

    fn update(&mut self, bytes: impl AsRef<[u8]>) {
        for b in bytes.as_ref() {
            self.updates[self.position] = *b;
            self.position += 1;
        }
    }

    fn chain(self, _data: impl AsRef<[u8]>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn finalize(self) -> Output<Self> {
        self.updates
    }

    fn finalize_reset(&mut self) -> Output<Self> {
        let output = self.updates;
        self.updates = GenericArray::default();
        self.position = 0;
        output
    }

    fn reset(&mut self) {
        self.updates = GenericArray::default();
        self.position = 0;
    }

    fn output_size() -> usize {
        256
    }

    fn digest(data: &[u8]) -> Output<Self> {
        let mut myself = Self::new();
        myself.update(data);
        myself.finalize()
    }
}

#[test]
fn ion_hash_tests() -> IonResult<()> {
    test_file("tests/ion_hash_tests.ion")
}

fn test_file(file_name: &str) -> IonResult<()> {
    let data = read(file_name)?;
    let elems = element_reader().read_all(&data)?;
    test_all(elems)
}

fn test_all<E: Element>(elems: Vec<E>) -> IonResult<()> {
    for case in &elems {
        let case = case.as_struct().expect("test cases are structs");
        // TODO: support binary ion
        let ion = case.get("ion").expect("test cases have an `ion` value");
        let expect = case
            .get("expect")
            .expect("test cases have an `expect` value");
        test_case(ion, expect)?;
    }

    Ok(())
}

fn test_case<E: Element>(ion: &E, strukt: &E) -> IonResult<()> {
    let strukt = strukt.as_struct().expect("`expect` should be a struct");
    let identity = strukt
        .get("identity")
        .expect("`expect` should have a field called `identity`")
        .as_sequence()
        .expect("`identity` should be a sexp");

    let digest = TestDigest::default();
    let hasher = IonHasher::new(digest.clone());
    let test_case_name = test_case_name(ion)?;
    let result = hasher.hash_element(ion)?;

    for it in identity.iter() {
        let method = it
            .annotations()
            .next()
            .expect("identity sexps have one annotation")
            .text()
            .expect("identity sexps contain elements with text annotations");

        let bytes: Vec<_> = it
            .as_sequence()
            .expect("identity sexps have sub-sexps")
            .iter()
            .map(|it| it.as_i64().expect("sub-exps have bytes") as u8)
            .collect();

        match method {
            "update" => {
                // TODO: We currently don't assert on intermediate updates. It's
                // not clear if this is actually valuable, other than helping
                // diagnose bugs.
            }
            "digest" => {
                // Because `TestDigest` uses generic array (fixed size,
                // intialized with zeros) and isn't doing real hashing, we land
                // up an array that has our result filled in to N bytes and then
                // a bunch of trailing zeros. We want to ignore those, since
                // they're not relevant.
                let ignore_trailing_zeros = result
                    .iter()
                    .rposition(|b| *b != 0)
                    .map(|p| p + 1)
                    .unwrap_or(bytes.len());
                let result_slice = &result[0..ignore_trailing_zeros];

                // Convert into hex repr to make assertion failures look like
                // the test case definitions.
                let expected = format!("{:02X?}", bytes);
                let actual = format!("{:02X?}", result_slice);

                assert_eq!(
                    expected, actual,
                    "case: {}; bytes failed to match",
                    test_case_name
                );
            }
            other => unimplemented!("{} is not yet implemented", other),
        }
    }

    Ok(())
}

/// Test cases may be annotated with a test name. Or, not! If they aren't, the
/// name of the test is the Ion text representation of the input value.
// TODO: Once `dumper` lands, use it to generate test names for un-annotated
// test cases. For now, they're simply numbered.
fn test_case_name<E: Element>(ion: &E) -> IonResult<String> {
    let annotations: Vec<_> = ion
        .annotations()
        .map(|it| it.text().unwrap().to_string())
        .collect();
    match &annotations[..] {
        [] => {
            use ion_rs::value::writer::{ElementWriter, Format, TextKind};

            let mut buf = vec![0u8; 4096];
            let mut writer = Format::Text(TextKind::Compact).element_writer_for_slice(&mut buf)?;
            writer.write(ion)?;
            let result = writer.finish()?;

            Ok(String::from_utf8_lossy(result).to_string())
        }
        [single] => Ok(single.clone()),
        _ => unimplemented!(),
    }
}