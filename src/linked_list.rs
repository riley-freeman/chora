use std::mem;

pub struct LinkedList<T> {
    first: *mut LinkedNode<T>,
    last: *mut LinkedNode<T>,
    length: usize,
}

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
        self.last  = raw;
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

    pub fn pinch(&mut self, ptr: *mut LinkedNode<T>) -> Option<T> {
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

pub struct LinkedNode<T> {
    inner: T,
    next: *mut LinkedNode<T>,
    prev: *mut LinkedNode<T>,
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
        let mut list = LinkedList::<i32>::new();
        assert_eq!(list.pinch(std::ptr::null_mut()), None);

        let node1 = list.push_back(10);
        let node2 = list.push_back(20);
        let node3 = list.push_back(30);

        assert_eq!(list.pinch(node2), Some(20));
        assert_eq!(list.length, 2);
        assert_eq!(unsafe { (*node1).next }, node3);
        assert_eq!(unsafe { (*node3).prev }, node1);

        assert_eq!(list.pinch(node3), Some(30));
        assert_eq!(list.length, 1);
        assert!(unsafe { (*node1).next.is_null() });

        assert_eq!(list.pinch(node1), Some(10));
        assert_eq!(list.length, 0);
        assert!(list.first.is_null());
        assert!(list.last.is_null());
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



