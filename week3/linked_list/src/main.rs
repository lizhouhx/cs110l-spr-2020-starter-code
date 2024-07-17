use linked_list::{ComputeNorm, LinkedList};
pub mod linked_list;

fn main() {
    let mut list: LinkedList<String> = LinkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.get_size(), 0);
    for i in 1..12 {
        list.push_front(i.to_string());
    }
    println!("{}", list);
    println!("list size: {}", list.get_size());
    println!("top element: {}", list.pop_front().unwrap());
    println!("{}", list);
    println!("size: {}", list.get_size());
    println!("{}", list.to_string()); // ToString impl for anything impl Display
    println!("list_clone is {}",list.clone());

    //If you implement iterator trait:
    for val in &list {
       println!("iterator test: {}", val);
    }

    //Test compute_norm
    let mut f64_list:LinkedList<f64> = LinkedList::new();
    f64_list.push_front(3.0);
    f64_list.push_front(4.0);
    f64_list.push_front(5.0);
    println!("f64_list's compute_norm result is {}", f64_list.compute_norm());
}
