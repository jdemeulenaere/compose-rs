//! `AnimationVector` against Compose's `AnimationVector1D` to `AnimationVector4D`.

use compose_animation::AnimationVector;

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationVectorTest.kt, testReset
#[test]
fn a_reset_vector_is_zeros_of_its_size() {
    for size in 1..=4 {
        let mut vector = AnimationVector::zero(size).map(|index, _| index as f32 + 1.0);
        vector.reset();
        assert_eq!(vector, AnimationVector::zero(size));
        assert_eq!(vector.size(), size);
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimationVectorTest.kt, testAnimationVectorFactoryMethod
#[test]
fn a_vector_made_from_floats_has_them() {
    assert_eq!(AnimationVector::from(200.0).as_slice(), &[200.0]);
    assert_eq!(AnimationVector::from([7.0, 500.0]).as_slice(), &[7.0, 500.0]);
    assert_ne!(AnimationVector::from([7.0, 501.0]), AnimationVector::from([7.0, 500.0]));
    assert_eq!(AnimationVector::from([35.0, 26.0, 50.0]).as_slice(), &[35.0, 26.0, 50.0]);
    assert_eq!(AnimationVector::from([1.0, 2.0, 3.0, 4.0]).as_slice(), &[1.0, 2.0, 3.0, 4.0]);
    // Sizes differ: not equal, though the components shared are.
    assert_ne!(AnimationVector::from(1.0), AnimationVector::from([1.0, 0.0]));
}
