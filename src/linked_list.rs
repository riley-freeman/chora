use std::mem;
use std::ptr::null_mut;

#[derive(Default)]
pub struct LinkedList<T> {
    first: *mut LinkedNode<T>,
    last: *mut LinkedNode<T>,
    length: usize,
}

// TODO: Finish implementing LinkedList
impl <T> LinkedList<T> {
    pub fn new() -> Self {
        LinkedList {
            length: 0,
            first: std::ptr::null_mut(),
            last: std::ptr::null_mut(),
        }
    }

    pub fn push_front(&mut self, data: T) -> *mut LinkedNode<T> {
        let boxed = Box::new(LinkedNode {
            inner: data,
            next: self.first,
            prev: std::ptr::null_mut(),
        });
        let raw: *mut LinkedNode<T> = Box::into_raw(boxed);

        // TODO: Check back at this part... its *prolly* wrong
        if !self.first.is_null() {
            let first = unsafe { &mut *self.first };
            first.prev = raw;
        }

        self.length += 1;
        self.first = raw;
        if self.last.is_null() {
            self.last = raw;
        }

        // Return a pointer directly to the node
        raw
    }

    pub fn push_back(&mut self, data: T) -> *mut LinkedNode<T> {
        let boxed = Box::new(LinkedNode {
            inner: data,
            next: std::ptr::null_mut(),
            prev: self.last,
        });
        let raw: *mut LinkedNode<T> = Box::into_raw(boxed);

        // TODO: Check back this part too...
        if !self.last.is_null() {
            let last = unsafe { &mut *self.last };
            last.next = raw;
        }

        self.length += 1;
        self.last = raw;
        if self.first.is_null() {
            self.first = raw;
        }

        // Return a pointer directly to the node
        raw
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if !self.first.is_null() {
            // Check if it's also the last one
            if self.first == self.last { self.last = std::ptr::null_mut() }

            let first = unsafe { Box::from_raw(self.first) };
            self.first = first.next;

            // Update the new first node
            if !self.first.is_null() {
                let first = unsafe { &mut *self.first };
                first.prev = std::ptr::null_mut();
            }

            self.length -= 1;
            Some(first.inner)
        } else {
            None
        }
    }

    pub fn pop_back(&mut self) -> Option<T> {
        if !self.last.is_null() {
            // Check if it's also the first one
            if self.first == self.last { self.first = std::ptr::null_mut() }

            let last = unsafe { Box::from_raw(self.last) };
            self.last = last.prev;

            // Update the new last node
            if !self.last.is_null() {
                let last = unsafe { &mut *self.last };
                last.next = std::ptr::null_mut();
            }

            self.length -= 1;
            Some(last.inner)
        } else {
            None
        }
    }

    pub unsafe fn pinch(&mut self, ptr: *mut LinkedNode<T>) -> Option<T> {
        if ptr.is_null() { return None }

        if ptr == self.first {
            self.pop_front()
        } else if ptr == self.last {
            self.pop_back()
        } else {
            let node = unsafe { Box::from_raw(ptr) };

            let prev = unsafe { &mut* node.prev };
            let next = unsafe { &mut* node.next };

            prev.next = node.next;
            next.prev = node.prev;
            self.length -= 1;

            Some(node.inner)
        }
    }

    pub unsafe fn merge(&mut self, right: LinkedList<T>) {
        if self.length == 0 {
            *self = right;
            return;
        }

        self.length += right.len();

        let last = unsafe { &mut *self.last };
        last.next = right.first;
        self.last = right.last;
    }

    pub fn len(&self) -> usize {
        self.length
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut current = self.first;
        while !current.is_null() {
            let boxed = unsafe { Box::from_raw(current) };
            current = boxed.next;
            mem::drop(boxed);
        }
    }
}

impl<T: Clone> Clone for LinkedList<T> {
    fn clone(&self) -> Self {
        let mut new_list = LinkedList::new();

        // If empty, return an empty list
        if self.first.is_null() {
            return new_list;
        }

        // Iterate through nodes and clone each one
        let mut current = self.first;
        while !current.is_null() {
            let node = unsafe { &*current };
            new_list.push_back(node.inner.clone());
            current = node.next;
        }

        new_list
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for LinkedList<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();

        let mut current = self.first;
        while !current.is_null() {
            let node = unsafe { &*current };
            list.entry(&node.inner);
            current = node.next;
        }

        list.finish()
    }
}

impl<T: PartialEq> PartialEq for LinkedList<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.length != other.length {
            return false;
        }

        let mut self_current = self.first;
        let mut other_current = other.first;

        while !self_current.is_null() && !other_current.is_null() {
            let self_node = unsafe { &*self_current };
            let other_node = unsafe { &*other_current };

            if self_node.inner != other_node.inner {
                return false;
            }

            self_current = self_node.next;
            other_current = other_node.next;
        }

        self_current.is_null() && other_current.is_null()
    }
}

impl<T: Eq> Eq for LinkedList<T> {}

impl<T: PartialOrd> PartialOrd for LinkedList<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let mut self_current = self.first;
        let mut other_current = other.first;

        while !self_current.is_null() && !other_current.is_null() {
            let self_node = unsafe { &*self_current };
            let other_node = unsafe { &*other_current };

            match self_node.inner.partial_cmp(&other_node.inner) {
                Some(std::cmp::Ordering::Equal) => {
                    self_current = self_node.next;
                    other_current = other_node.next;
                }
                non_equal => return non_equal,
            }
        }

        // If we've exhausted one list but not the other, the shorter is "less"
        if self_current.is_null() && other_current.is_null() {
            Some(std::cmp::Ordering::Equal)
        } else if self_current.is_null() {
            Some(std::cmp::Ordering::Less)
        } else {
            Some(std::cmp::Ordering::Greater)
        }
    }
}

impl<T: Ord> Ord for LinkedList<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let mut self_current = self.first;
        let mut other_current = other.first;

        while !self_current.is_null() && !other_current.is_null() {
            let self_node = unsafe { &*self_current };
            let other_node = unsafe { &*other_current };

            match self_node.inner.cmp(&other_node.inner) {
                std::cmp::Ordering::Equal => {
                    self_current = self_node.next;
                    other_current = other_node.next;
                }
                non_equal => return non_equal,
            }
        }

        // If we've exhausted one list but not the other, the shorter is "less"
        if self_current.is_null() && other_current.is_null() {
            std::cmp::Ordering::Equal
        } else if self_current.is_null() {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    }
}

impl<T, const N: usize> From<[T; N]> for LinkedList<T> {
    fn from(arr: [T; N]) -> Self {
        Self::from_iter(arr)
    }
}

impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item=T>>(iter: I) -> Self {
        let mut list = LinkedList::new();
        for item in iter {
            list.push_back(item);
        }
        list
    }
}

// Explicitly implement Send for LinkedList<T> when T is Send
unsafe impl<T: Send> Send for LinkedList<T> {}

// Explicitly implement Sync for LinkedList<T> when T is Sync
unsafe impl<T: Sync> Sync for LinkedList<T> {}

// Iterator types and methods for LinkedList
impl<T> LinkedList<T> {
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.first,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            next: self.first,
            phantom: std::marker::PhantomData,
        }
    }
}

// Immutable iterator for LinkedList
pub struct Iter<'a, T> {
    next: *mut LinkedNode<T>,
    phantom: std::marker::PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.is_null() {
            None
        } else {
            // Safety: next is non-null and comes from a valid LinkedList
            let node = unsafe { &*self.next };
            self.next = node.next;
            // Safety: the lifetime of the reference is tied to the lifetime of the LinkedList
            Some(unsafe { &*(&node.inner as *const T) })
        }
    }
}

// Mutable iterator for LinkedList
pub struct IterMut<'a, T> {
    next: *mut LinkedNode<T>,
    phantom: std::marker::PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.is_null() {
            None
        } else {
            // Safety: next is non-null and comes from a valid LinkedList
            let node = unsafe { &mut *self.next };
            self.next = node.next;
            // Safety: the lifetime of the reference is tied to the lifetime of the LinkedList
            Some(unsafe { &mut *(&mut node.inner as *mut T) })
        }
    }
}

// Consuming iterator for LinkedList
pub struct IntoIter<T> {
    list: LinkedList<T>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.list.pop_front()
    }
}

impl<'a, T> IntoIterator for &'a LinkedList<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut LinkedList<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> IntoIterator for LinkedList<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter { list: self }
    }
}


impl<T> FromIterator<T> for LinkedList<T> {
    fn from_iter<I: IntoIterator<Item=T>>(iter: I) -> Self {
        let mut list = LinkedList::new();
        for item in iter {
            list.push_back(item);
        }
        list
    }
}


impl<T> Extend<T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item);
        }
    }
}

impl<'a, T: Clone + 'a> Extend<&'a T> for LinkedList<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, iter: I) {
        for item in iter {
            self.push_back(item.clone());
        }
    }
}

impl<T: std::hash::Hash> std::hash::Hash for LinkedList<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.length.hash(state);

        // Hash each element in order
        let mut current = self.first;
        while !current.is_null() {
            let node = unsafe { &*current };
            node.inner.hash(state);
            current = node.next;
        }
    }
}

pub struct LinkedNode<T> {
    inner: T,
    next: *mut LinkedNode<T>,
    prev: *mut LinkedNode<T>,
}

impl<T: Clone> Clone for LinkedNode<T> {
    fn clone(&self) -> Self {
        LinkedNode {
            inner: self.inner.clone(),
            next: self.next,
            prev: self.prev,
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for LinkedNode<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkedNode")
            .field("inner", &self.inner)
            .field("next", &(self.next as usize))
            .field("prev", &(self.prev as usize))
            .finish()
    }
}

impl<T: PartialEq> PartialEq for LinkedNode<T> {
    fn eq(&self, other: &Self) -> bool {
        // Only compare the values, not the pointers
        self.inner == other.inner
    }
}

impl<T: Eq> Eq for LinkedNode<T> {}

impl<T: PartialOrd> PartialOrd for LinkedNode<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.inner.partial_cmp(&other.inner)
    }
}

impl<T: Ord> Ord for LinkedNode<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

// Explicitly implement Send for LinkedNode<T> when T is Send
unsafe impl<T: Send> Send for LinkedNode<T> {}

// Explicitly implement Sync for LinkedNode<T> when T is Sync
unsafe impl<T: Sync> Sync for LinkedNode<T> {}

impl<T: std::hash::Hash> std::hash::Hash for LinkedNode<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash only the inner value, consistent with PartialEq implementation
        self.inner.hash(state);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let list: LinkedList<i32> = LinkedList::new();
        assert_eq!(list.length, 0);
        assert!(list.first.is_null());
        assert!(list.last.is_null());
    }

    #[test]
    fn test_push_front() {
        let mut list = LinkedList::new();
        let node1 = list.push_front(10);
        assert_eq!(list.length, 1);
        assert_eq!(unsafe { (*node1).inner }, 10);
        assert!(unsafe { (*node1).prev.is_null() });
        assert!(unsafe { (*node1).next.is_null() });

        let node2 = list.push_front(20);
        assert_eq!(list.length, 2);
        assert_eq!(unsafe { (*node2).inner }, 20);
        assert_eq!(unsafe { (*node2).next }, node1);
        assert_eq!(unsafe { (*node1).prev }, node2);
    }

    #[test]
    fn test_push_back() {
        let mut list = LinkedList::new();
        let node1 = list.push_back(10);
        assert_eq!(list.length, 1);
        assert_eq!(unsafe { (*node1).inner }, 10);
        assert!(unsafe { (*node1).prev.is_null() });
        assert!(unsafe { (*node1).next.is_null() });

        let node2 = list.push_back(20);
        assert_eq!(list.length, 2);
        assert_eq!(unsafe { (*node2).inner }, 20);
        assert_eq!(unsafe { (*node2).prev }, node1);
        assert_eq!(unsafe { (*node1).next }, node2);
    }

    #[test]
    fn test_pop_front() {
        let mut list = LinkedList::new();
        assert_eq!(list.pop_front(), None);

        list.push_back(10);
        list.push_back(20);
        assert_eq!(list.pop_front(), Some(10));
        assert_eq!(list.length, 1);
        assert!(unsafe { (*list.first).prev.is_null() });
        assert_eq!(list.pop_front(), Some(20));
        assert_eq!(list.length, 0);
        assert!(list.first.is_null());
        assert!(list.last.is_null());
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn test_pop_back() {
        let mut list = LinkedList::new();
        assert_eq!(list.pop_back(), None);

        list.push_front(10);
        list.push_front(20);
        assert_eq!(list.pop_back(), Some(10));
        assert_eq!(list.length, 1);
        assert!(unsafe { (*list.last).next.is_null() });
        assert_eq!(list.pop_back(), Some(20));
        assert_eq!(list.length, 0);
        assert!(list.first.is_null());
        assert!(list.last.is_null());
        assert_eq!(list.pop_back(), None);
    }

    #[test]
    fn test_single_node() {
        let mut list = LinkedList::new();
        let node = list.push_front(42);
        assert_eq!(list.length, 1);
        assert_eq!(list.first, node);
        assert_eq!(list.last, node);
        assert!(unsafe { (*node).next.is_null() });
        assert!(unsafe { (*node).prev.is_null() });

        // Remove the only node
        assert_eq!(list.pop_front(), Some(42));
        assert_eq!(list.length, 0);
        assert!(list.first.is_null());
        assert!(list.last.is_null());
    }

    #[test]
    fn test_pinch() {
        unsafe {
            let mut list = LinkedList::<i32>::new();
            assert_eq!(list.pinch(std::ptr::null_mut()), None);

            let node1 = list.push_back(10);
            let node2 = list.push_back(20);
            let node3 = list.push_back(30);

            assert_eq!(list.pinch(node2), Some(20));
            assert_eq!(list.length, 2);
            assert_eq!((*node1).next, node3);
            assert_eq!((*node3).prev, node1);

            assert_eq!(list.pinch(node3), Some(30));
            assert_eq!(list.length, 1);
            assert!((*node1).next.is_null());

            assert_eq!(list.pinch(node1), Some(10));
            assert_eq!(list.length, 0);
            assert!(list.first.is_null());
            assert!(list.last.is_null());
        }
    }

    #[test]
    fn test_generic_support() {
        let mut list = LinkedList::new();
        list.push_front("Hello");
        list.push_back("World");
        assert_eq!(list.pop_front(), Some("Hello"));
        assert_eq!(list.pop_back(), Some("World"));
    }

    #[test]
    fn test_complex_operations() {
        let mut list = LinkedList::new();
        list.push_back(10);
        list.push_front(20);
        list.push_back(30);

        assert_eq!(list.pop_front(), Some(20));
        assert_eq!(list.pop_back(), Some(30));
        assert_eq!(list.pop_front(), Some(10));
        assert!(list.first.is_null());
        assert!(list.last.is_null());
        assert_eq!(list.length, 0);

        list.push_back(40);
        assert_eq!(list.pop_back(), Some(40));
    }
}
