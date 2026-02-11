use tokio::task::JoinHandle;

pub struct AbortOnDrop<T>(pub JoinHandle<T>);
impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
