use crate::error::PolarsError;
use crate::error::Result;
use arrow::array::{Array, ArrayRef};
use arrow::datatypes::DataType;
use arrow::{
    array,
    array::{PrimitiveArray, PrimitiveBuilder},
    compute, datatypes,
    datatypes::{ArrowNumericType, ArrowPrimitiveType, Field, Int8Type},
};
use num::Zero;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::ops::{Add, Div, Mul, Sub};
use std::rc::Rc;
use std::sync::Arc;

struct ChunkedArray<T> {
    field: Field,
    // For now settle with dynamic generics until we are more confident about the api
    chunks: Vec<ArrayRef>,
    /// sum of all chunk lengths
    len: usize,
    /// sum of all chunk nulls
    null_counts: usize,
    phantom: PhantomData<T>,
}

impl<T> ChunkedArray<T>
where
    T: ArrowPrimitiveType,
{
    fn new<K>(name: &str, v: &[K::Native]) -> Self
    where
        K: ArrowPrimitiveType,
    {
        let mut builder = PrimitiveBuilder::<K>::new(v.len());
        v.into_iter().for_each(|&val| {
            builder.append_value(val).expect("Could not append value");
        });

        let field = Field::new(name, K::get_data_type(), true);

        ChunkedArray {
            field,
            chunks: vec![Arc::new(builder.finish())],
            len: v.len(),
            null_counts: 0,
            phantom: PhantomData,
        }
    }
}

impl<T> ChunkedArray<T> {
    fn copy_with_array(&self, arr: Vec<ArrayRef>) -> Self {
        ChunkedArray {
            field: self.field.clone(),
            chunks: arr,
            len: self.len,
            null_counts: self.null_counts,
            phantom: PhantomData,
        }
    }
    //
    // /// Caller determines the data type, and only works on single chunked series.
    // fn get_iter<T: ArrowNumericType>(&self) -> impl Iterator<Item = &T::Native> + '_ {
    //     let a0_any = self.chunks[0].as_any();
    //     let arr = a0_any
    //         .downcast_ref::<PrimitiveArray<T>>()
    //         .expect("could not downcast");
    //     let slice = arr.value_slice(0, arr.len());
    //     slice.iter()
    // }
}

macro_rules! variant_operand {
    ($_self:expr, $rhs:tt, $data_type:ty, $operand:ident, $expect:expr) => {{
        let mut new_chunks = Vec::with_capacity($_self.chunks.len());
        $_self
            .chunks
            .iter()
            .zip($rhs.chunks.iter())
            .for_each(|(l, r)| {
                let left_any = l.as_any();
                let right_any = r.as_any();
                let left = left_any
                    .downcast_ref::<PrimitiveArray<$data_type>>()
                    .unwrap();
                let right = right_any
                    .downcast_ref::<PrimitiveArray<$data_type>>()
                    .unwrap();
                let res =
                    Arc::new(arrow::compute::$operand(left, right).expect($expect)) as ArrayRef;
                new_chunks.push(res);
            });
        $_self.copy_with_array(new_chunks)
    }};
}

impl<T> Add for &ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = ChunkedArray<T>;

    fn add(self, rhs: Self) -> Self::Output {
        let expect_str = "Could not add, check data types and length";
        variant_operand![self, rhs, T, add, expect_str]
    }
}

impl<T> Mul for &ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = ChunkedArray<T>;

    fn mul(self, rhs: Self) -> Self::Output {
        let expect_str = "Could not multiply, check data types and length";
        variant_operand!(self, rhs, T, multiply, expect_str)
    }
}

impl<T> Sub for &ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = ChunkedArray<T>;

    fn sub(self, rhs: Self) -> Self::Output {
        let expect_str = "Could not subtract, check data types and length";
        variant_operand![self, rhs, T, subtract, expect_str]
    }
}

impl<T> Add for ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        (&self).add(&rhs)
    }
}

impl<T> Mul for ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        (&self).mul(&rhs)
    }
}

impl<T> Sub for ChunkedArray<T>
where
    T: ArrowNumericType,
    T::Native: Add<Output = T::Native>
        + Sub<Output = T::Native>
        + Mul<Output = T::Native>
        + Div<Output = T::Native>
        + num::Zero,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        (&self).sub(&rhs)
    }
}

impl<T> Debug for ChunkedArray<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{:?}", self.chunks))
    }
}

impl<T> Clone for ChunkedArray<T> {
    fn clone(&self) -> Self {
        ChunkedArray {
            field: self.field.clone(),
            chunks: self.chunks.clone(),
            len: self.len,
            null_counts: self.null_counts,
            phantom: PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn get_array() -> ChunkedArray<datatypes::Int32Type> {
        ChunkedArray::new::<datatypes::Int32Type>("a", &[1, 2, 3])
    }

    #[test]
    fn arithmetic() {
        let s1 = get_array();
        println!("{:?}", s1.chunks);
        let s2 = &s1.clone();
        let s1 = &s1;
        println!("{:?}", s1 + s2);
        println!("{:?}", s1 - s2);
        println!("{:?}", s1 * s2);
    }

    // #[test]
    // fn iter() {
    //     let s1 = get_array();
    //     let mut a = s1.get_iter::<datatypes::Int32Type>();
    //     let v = a.next().unwrap();
    //
    //     println!("{:?}", v)
    // }
}
