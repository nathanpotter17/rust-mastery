use std::ops::Add;

// Choosing a type at compile time

// The type a total of many Selfs is computed in. A total of many i32s can pass i32::MAX even when every value fits, so it needs a wider type of the same signedness.
pub trait Wide: Copy + Ord {
    type Total: Copy + Default + From<Self> + Add<Output = Self::Total>;
}

impl Wide for i32 {
    type Total = i64;
}

impl Wide for u32 {
    type Total = u64;
}

pub type TotalOf<T> = <T as Wide>::Total;

// The summary

pub struct Summary<T: Wide> {
    pub count: usize,
    pub min: T,
    pub median: T,
    pub max: T,
    pub total: TotalOf<T>,
}

// Accepts anything that can be iterated over and yields numbers: a Vec, an array, or a chain of iterator adapters. None if there are no values.
pub fn summarize<I>(values: I) -> Option<Summary<I::Item>>
where
    I: IntoIterator,
    I::Item: Wide,
{
    // Our own sorted copy, whatever we were given. collect() builds the collection the variable's type names.
    let mut sorted: Vec<I::Item> = values.into_iter().collect();
    sorted.sort_unstable();

    // first() is None for an empty Vec, and ? passes that on.
    let min = *sorted.first()?;
    let max = *sorted.last()?;

    // fold combines the values left to right, starting from the total type's zero. Each value is widened with From before it's added.
    let total = sorted.iter().fold(TotalOf::<I::Item>::default(), |sum, &value| sum + value.into());

    // With an even count we take the upper of the two middle values.
    Some(Summary { count: sorted.len(), min, median: sorted[sorted.len() / 2], max, total })
}
