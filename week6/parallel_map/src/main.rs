use std::sync::mpsc;
use std::{thread, time};

fn parallel_map<T, U, F>(mut input_vec: Vec<T>, num_threads: usize, f: F) -> Vec<U>
where
    F: FnOnce(T) -> U + Send + Copy + 'static,
    T: Send + 'static,
    U: Send + 'static + Default,
{
    let mut output_vec: Vec<U> = Vec::with_capacity(input_vec.len());
    unsafe{output_vec.set_len(input_vec.len())}
    let mut threads = Vec::new();
    let (sender, receiver) = mpsc::channel();
    let (s, r) = mpsc::channel();  
    while !input_vec.is_empty(){
        let idx = input_vec.len() - 1;
        let value = input_vec.pop().unwrap();
        s.send((idx, value)).expect("Found no r");
    }
    drop(s);
    for _ in 0..num_threads{  
        while let Ok((idx, value)) = r.recv(){
            let sender = sender.clone();
            threads.push(thread::spawn(move ||{ 
                    sender.send((idx, f(value))).expect("Found no receiver");                                 
            }))
        }           
    }
    drop(sender);   
    while let Ok((idx, value)) = receiver.recv(){
        output_vec[idx] = value;
    }   
    for thread in threads{
        thread.join().expect("Panic occurred in threads");
    }
    output_vec
}

fn main() {
    let v = vec![6, 7, 8, 9, 10, 1, 2, 3, 4, 5, 12, 18, 11, 5, 20];
    let squares = parallel_map(v, 10, |num| {
        println!("{} squared is {}", num, num * num);
        thread::sleep(time::Duration::from_millis(500));
        num * num
    });
    println!("squares: {:?}", squares);
}
