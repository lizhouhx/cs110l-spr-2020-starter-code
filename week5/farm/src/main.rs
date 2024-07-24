use std::{cell::RefCell, collections::VecDeque};
#[allow(unused_imports)]
use std::sync::{Arc, Mutex};
use std::time::Instant;
#[allow(unused_imports)]
use std::{env, process, thread};

/// Determines whether a number is prime. This function is taken from CS 110 factor.py.
///
/// You don't need to read or understand this code.
#[allow(dead_code)]
fn is_prime(num: u32) -> bool {
    if num <= 1 {
        return false;
    }
    for factor in 2..((num as f64).sqrt().floor() as u32) {
        if num % factor == 0 {
            return false;
        }
    }
    true
}

/// Determines the prime factors of a number and prints them to stdout. This function is taken
/// from CS 110 factor.py.
///
/// You don't need to read or understand this code.
#[allow(dead_code)]
fn factor_number(num: u32) {
    let start = Instant::now();

    if num == 1 || is_prime(num) {
        println!("{} = {} [time: {:?}]", num, num, start.elapsed());
        return;
    }

    let mut factors = Vec::new();
    let mut curr_num = num;
    for factor in 2..num {
        while curr_num % factor == 0 {
            factors.push(factor);
            curr_num /= factor;
        }
    }
    factors.sort();
    let factors_str = factors
        .into_iter()
        .map(|f| f.to_string())
        .collect::<Vec<String>>()
        .join(" * ");
    println!("{} = {} [time: {:?}]", num, factors_str, start.elapsed());
}

/// Returns a list of numbers supplied via argv.
#[allow(dead_code)]
fn get_input_numbers() -> VecDeque<u32> {
    let mut numbers = VecDeque::new();
    for arg in env::args().skip(1) {
        if let Ok(val) = arg.parse::<u32>() {
            numbers.push_back(val);
        } else {
            println!("{} is not a valid number", arg);
            process::exit(1);
        }
    }
    numbers
}

fn get_number(vec: Arc<Mutex<VecDeque<u32>>>) -> Option<u32>{
    let mut num_mutex = vec.lock().unwrap();
    (*num_mutex).pop_front()
}

fn main() {
    let num_threads = num_cpus::get();
    println!("Farm starting on {} CPUs", num_threads);
    let start = Instant::now();

    // TODO: call get_input_numbers() and store a queue of numbers to factor
    let numbers_list = get_input_numbers();
    let numbers = Arc::new(Mutex::new(numbers_list));
    // TODO: spawn `num_threads` threads, each of which pops numbers off the queue and calls
    // factor_number() until the queue is empty
    let mut num_threads = Vec::new();
    for i in 0..10{
        let remain_numbers = numbers.clone();
        num_threads.push(thread::spawn(move ||{           
            let num = get_number(remain_numbers);
            match num {
                Some(_) => factor_number(num.unwrap()),
                None => ()
            }                        
        }));
    }
    // TODO: join all the threads you created
    for handle in num_threads{
        handle.join().expect("Panic happened!");
    }
    println!("Total execution time: {:?}", start.elapsed());
}
