use hash::{Hashable, Algorithm};
use proof::Proof;
use std::fmt::Debug;
use std::hash::Hasher;

/// Merkle Tree.
///
/// All leafs and nodes are stored in a linear array (vec).
///
/// A merkle tree is a tree in which every non-leaf node is the hash of its
/// children nodes. A diagram depicting how it works:
///
/// ```text
///         root = h1234 = h(h12 + h34)
///        /                           \
///  h12 = h(h1 + h2)            h34 = h(h3 + h4)
///   /            \              /            \
/// h1 = h(tx1)  h2 = h(tx2)    h3 = h(tx3)  h4 = h(tx4)
/// ```
///
/// In memory layout:
///
/// ```text
///     [h1 h2 h3 h4 h12 h34 root]
/// ```
///
/// Merkle root is always the last element in the array.
///
/// The number of inputs is not always a power of two which results in a
/// balanced tree structure as above.  In that case, parent nodes with no
/// children are also zero and parent nodes with only a single left node
/// are calculated by concatenating the left node with itself before hashing.
/// Since this function uses nodes that are pointers to the hashes, empty nodes
/// will be nil.
///
/// TODO: From<> trait impl?
/// TODO: Index<t>
/// TODO: Ord, Eq
/// TODO: Customizable merkle hash helper
/// TODO: replace Vec with raw mem one day
/// TODO: Deref<T> plz for as_slice and len
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MerkleTree<T: Ord + Clone + Default + Debug, A: Algorithm<T>> {
    data: Vec<T>,
    olen: usize,
    leafs: usize,
    height: usize,
    alg: A,
}

impl<T: Ord + Clone + Default + Debug, A: Algorithm<T> + Hasher + Clone> MerkleTree<T, A> {
    /// Creates new merkle from a sequence of hashes.
    pub fn new(data: &[T], alg: A) -> MerkleTree<T, A> {
        Self::from_hash(data, alg)
    }

    /// Creates new merkle from a sequence of hashes.
    pub fn from_hash(data: &[T], alg: A) -> MerkleTree<T, A> {
        Self::from_iter(data.iter().map(|x| x.clone()), alg)
    }

    /// Creates new merkle tree from a list of hashable objects.
    pub fn from_data<O: Hashable<A>>(data: &[O], a: A) -> MerkleTree<T, A> {
        let mut b = a.clone();
        Self::from_iter(
            data.iter().map(|x| {
                b.reset();
                x.hash(&mut b);
                b.hash()
            }),
            a,
        )
    }

    /// Creates new merkle tree from an iterator over hashable objects.
    pub fn from_iter<I: IntoIterator<Item = T>>(into: I, alg: A) -> MerkleTree<T, A> {
        let iter = into.into_iter();
        let iter_count = match iter.size_hint().1 {
            Some(e) => e,
            None => panic!("not supported / not implemented"),
        };
        assert!(iter_count > 1);

        let pow = next_pow2(iter_count);
        let size = 2 * pow - 1;

        let mut mt: MerkleTree<T, A> = MerkleTree {
            data: Vec::with_capacity(size),
            olen: iter_count,
            leafs: pow,
            height: log2_pow2(size + 1),
            alg,
        };

        // compute leafs
        for item in iter {
            mt.data.push(mt.alg.leaf(item))
        }

        mt.build();
        mt
    }

    fn build(&mut self) {
        let size = 2 * self.leafs - 1;
        let h0 = T::default();

        // not built yet
        debug_assert_ne!(size, self.data.len());

        // fill in
        for _ in 0..(size - self.olen) {
            self.data.push(h0.clone());
        }

        // build tree
        let mut i: usize = 0;
        let mut j: usize = (size + 1) / 2; // pow
        while i < size - 1 {
            if self.data[i] == h0 {
                // when there is no left child node, the parent is nil too.
                self.data[j] = h0.clone();
            } else if self.data[i + 1] == h0 {
                // when there is no right child, the parent is generated by
                // hashing the concatenation of the left child with itself.
                self.data[j] = self.alg.node(self.data[i].clone(), self.data[i].clone());
            } else {
                // the normal case sets the parent node to the double sha256
                // of the concatenation of the left and right children.
                self.data[j] = self.alg.node(
                    self.data[i].clone(),
                    self.data[i + 1].clone(),
                );
            }

            j += 1;
            i += 2;
        }
    }

    /// Generate merkle tree inclusion proof for leaf `i`
    pub fn gen_proof(&self, i: usize) -> Proof<T> {
        assert!(i < self.olen); // i in [0 .. self.valid_leafs)

        let mut base = 0;
        let mut step = self.leafs; // power of 2
        let mut j = i;

        let h0 = T::default();
        let mut lemma: Vec<T> = Vec::with_capacity(self.height + 1); // path + root
        let mut path: Vec<bool> = Vec::with_capacity(self.height - 1); // path - 1
        lemma.push(self.data[i].clone());

        while step > 1 {
            let pair = if j & 1 == 0 {
                // j is left
                let rh = base + j + 1;
                if self.data[rh] == h0 {
                    // right is empty
                    base + j
                } else {
                    // right is good
                    base + j + 1
                }
            } else {
                // j is right
                base + j - 1
            };
            lemma.push(self.data[pair].clone());
            path.push(j & 1 == 0);
            base += step;
            step >>= 1;
            j >>= 1;
        }

        // root is final
        lemma.push(self.root());
        Proof::new(lemma, path)
    }

    /// Returns merkle root
    pub fn root(&self) -> T {
        self.data[self.data.len() - 1].clone()
    }

    /// Returns original number of elements the tree was built upon.
    pub fn olen(&self) -> usize {
        self.olen
    }

    /// Returns number of elements in the tree.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns height of the tree
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns count of leafs in the tree
    pub fn leafs(&self) -> usize {
        self.leafs
    }

    /// Extracts a slice containing the entire vector.
    ///
    /// Equivalent to `&s[..]`.
    pub fn as_slice(&self) -> &[T] {
        self.data.as_slice()
    }
}

/// next_pow2 returns next highest power of two from a given number if
/// it is not already a power of two.
///
/// http://locklessinc.com/articles/next_pow2/
/// https://stackoverflow.com/questions/466204/rounding-up-to-next-power-of-2/466242#466242
pub fn next_pow2(mut n: usize) -> usize {
    n -= 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    n |= n >> 32;
    return n + 1;
}

/// find power of 2 of a number which is power of 2
pub fn log2_pow2(n: usize) -> usize {
    n.trailing_zeros() as usize
}
