use std::sync::{mpsc, Arc, Mutex};
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
    let vec = Arc::new(Mutex::new(input_vec));
    for i in 0..num_threads{        
        let sender = sender.clone();
        let vec_ref = vec.clone();
        threads.push(thread::spawn(move ||{
            loop{
                let idx:usize;
                let value:T;
                {
                    let mut vec = vec_ref.lock().unwrap();
                    if vec.is_empty(){
                        break;
                    }
                    idx = vec.len() - 1;
                    value = vec.pop().unwrap();
                }//unlock the mutex, let other threads get the data
                //println!("In thread: {}",i);//test multithreading
                sender.send((idx, f(value))).expect("Found no receiver");
            }                                          
        }))                  
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
